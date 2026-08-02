use anyhow::{bail, Context, Result};
use git2::{Oid, Repository, Signature, Time};

use crate::actor::Actor;
use crate::domain::fold::fold;
use crate::domain::id::{short, TaskId};
use crate::domain::op::{Operation, OpEnvelope};
use crate::domain::task::Task;

const REF_PREFIX: &str = "refs/tasks/";
const OPS_BLOB_NAME: &str = "ops.json";
const BLOB_FILEMODE: i32 = 0o100644;

/// Reserved task-ref id holding the event-sourced per-repo config (see `config::config_op`).
/// It lives under `refs/tasks/` so it syncs with tasks by the same refspecs, but `"config"` is
/// not a 40-hex oid so it can never collide with a real task id. `list_ids` and `resolve` skip
/// it so it never surfaces or folds as a task.
pub const CONFIG_ID: &str = "config";

/// Reads and writes tasks as event-sourced op-chains under `refs/tasks/<id>`.
/// `<id>` is the oid of the task's creation commit (the chain's root); the ref's
/// target advances to the tip commit as ops are appended. History is a DAG, not
/// strictly linear: a pull that reconciles two divergent chains writes a real
/// two-parent merge commit (see `merge`), so `load` topologically orders every
/// reachable commit (see `topological_order`) rather than assuming a single
/// parent per commit or trusting timestamps to order commits on their own —
/// two CLI calls routinely land in the same wall-clock second.
pub struct Store<'repo> {
    repo: &'repo Repository,
}

impl<'repo> Store<'repo> {
    pub fn new(repo: &'repo Repository) -> Self {
        Self { repo }
    }

    pub fn create(&self, author: &Actor, ops: Vec<Operation>) -> Result<TaskId> {
        let envelopes = envelope_all(author, ops);
        let bytes = serde_json::to_vec_pretty(&envelopes).context("serializing ops")?;
        let ts = envelopes.first().map(|e| e.timestamp).unwrap_or_else(now);
        let commit_oid = self.write_commit(OPS_BLOB_NAME, Some(&bytes), &[], author, ts, "create task")?;
        let id = commit_oid.to_string();

        let ref_name = format!("{REF_PREFIX}{id}");
        self.repo
            .reference(&ref_name, commit_oid, false, "git-task: create")
            .with_context(|| format!("creating ref {ref_name}"))?;

        Ok(id)
    }

    pub fn append(&self, id: &TaskId, author: &Actor, ops: Vec<Operation>) -> Result<()> {
        let tip = self.tip(id)?;
        let envelopes = envelope_all(author, ops);
        let message = op_summary(&envelopes);
        let bytes = serde_json::to_vec_pretty(&envelopes).context("serializing ops")?;
        let ts = envelopes.first().map(|e| e.timestamp).unwrap_or_else(now);
        let commit_oid = self.write_commit(OPS_BLOB_NAME, Some(&bytes), &[tip], author, ts, &message)?;
        self.set_ref(id, commit_oid, true)
    }

    /// Create-or-append a single-blob commit on `refs/tasks/<id>`, carrying `blob` under
    /// `blob_name`. Used for the reserved config chain (`CONFIG_ID`); tasks go through
    /// `create`/`append`, which additionally build op-summary commit messages.
    pub fn append_chain(
        &self,
        id: &str,
        author: &Actor,
        blob_name: &str,
        blob: &[u8],
        ts: i64,
        message: &str,
    ) -> Result<()> {
        match self.find_tip(id)? {
            Some(tip) => {
                let oid = self.write_commit(blob_name, Some(blob), &[tip], author, ts, message)?;
                self.set_ref(id, oid, true)
            }
            None => {
                let oid = self.write_commit(blob_name, Some(blob), &[], author, ts, message)?;
                self.set_ref(id, oid, false)
            }
        }
    }

    /// Resolves an id prefix (as typed by the user) to the full task id.
    /// Accepts a bare hash prefix or a `KEY-<hash prefix>` display address.
    pub fn resolve(&self, prefix: &str) -> Result<TaskId> {
        let prefix = crate::domain::id::normalize_ref_input(prefix);
        let mut matches: Vec<String> = Vec::new();
        let refs = self.repo.references_glob(&format!("{REF_PREFIX}*"))?;
        for r in refs {
            let r = r?;
            if let Some(name) = r.name() {
                if let Some(id) = name.strip_prefix(REF_PREFIX) {
                    if id == CONFIG_ID {
                        continue; // reserved config ref, not a task
                    }
                    if id.starts_with(prefix) {
                        matches.push(id.to_string());
                    }
                }
            }
        }

        match matches.len() {
            0 => bail!("no task matching '{prefix}'"),
            1 => Ok(matches.remove(0)),
            _ => {
                matches.sort();
                let list = matches.iter().map(|m| short(m)).collect::<Vec<_>>().join(", ");
                bail!("'{prefix}' is ambiguous, matches: {list}");
            }
        }
    }

    pub fn load(&self, id: &TaskId) -> Result<Task> {
        let mut ops: Vec<OpEnvelope> = Vec::new();
        for blob in self.read_chain(id, OPS_BLOB_NAME)? {
            let text = std::str::from_utf8(&blob)
                .with_context(|| format!("{OPS_BLOB_NAME} in task {id} is not valid utf-8"))?;
            let batch: Vec<OpEnvelope> = serde_json::from_str(text)
                .with_context(|| format!("parsing {OPS_BLOB_NAME} in task {id}"))?;
            ops.extend(batch);
        }

        fold(id, &ops)
    }

    /// Reads each reachable commit's `blob_name` blob in topological order, skipping commits
    /// that don't carry it (e.g. two-parent merge commits). Shared by task `load` and config
    /// loading so both fold their ops in the identical DAG order (`topological_order`).
    pub fn read_chain(&self, id: &str, blob_name: &str) -> Result<Vec<Vec<u8>>> {
        let tip = self.tip(id)?;
        let order = self.topological_order(tip)?;

        let mut blobs = Vec::new();
        for oid in order {
            let commit = self.repo.find_commit(oid)?;
            let tree = commit.tree()?;
            let Some(entry) = tree.get_name(blob_name) else {
                continue;
            };
            let blob = entry
                .to_object(self.repo)?
                .into_blob()
                .map_err(|_| anyhow::anyhow!("{blob_name} is not a blob in commit {oid}"))?;
            blobs.push(blob.content().to_vec());
        }
        Ok(blobs)
    }

    /// Orders every commit reachable from `tip` so a commit always comes after all of
    /// its ancestors (Kahn's algorithm) — true causal order from the DAG itself, not
    /// timestamps. Two rapid CLI calls routinely land in the same wall-clock second,
    /// so a commit's own oid has no relation to when it was actually written; a plain
    /// timestamp sort silently reordered same-second commits and could put an edit
    /// before the CreateTask it depends on. Timestamp (then oid, for full
    /// determinism) only breaks ties between commits on genuinely different branches
    /// that have no ancestor relationship — never between a commit and its own parent.
    fn topological_order(&self, tip: Oid) -> Result<Vec<Oid>> {
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashMap};

        let mut revwalk = self.repo.revwalk()?;
        revwalk.push(tip)?;

        let mut commits: HashMap<Oid, git2::Commit> = HashMap::new();
        for oid in revwalk {
            let oid = oid?;
            commits.insert(oid, self.repo.find_commit(oid)?);
        }

        let mut indegree: HashMap<Oid, usize> = HashMap::new();
        let mut children: HashMap<Oid, Vec<Oid>> = HashMap::new();
        for (&oid, commit) in &commits {
            let parents: Vec<Oid> = commit.parent_ids().filter(|p| commits.contains_key(p)).collect();
            indegree.insert(oid, parents.len());
            for parent in parents {
                children.entry(parent).or_default().push(oid);
            }
        }

        let ready_key = |oid: Oid| -> Reverse<(i64, String, Oid)> {
            Reverse((commits[&oid].time().seconds(), oid.to_string(), oid))
        };

        let mut ready: BinaryHeap<Reverse<(i64, String, Oid)>> = indegree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&oid, _)| ready_key(oid))
            .collect();

        let mut order = Vec::with_capacity(commits.len());
        while let Some(Reverse((_, _, oid))) = ready.pop() {
            order.push(oid);
            if let Some(kids) = children.get(&oid) {
                for &child in kids {
                    let deg = indegree.get_mut(&child).expect("child was indexed above");
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push(ready_key(child));
                    }
                }
            }
        }

        Ok(order)
    }

    pub fn list_ids(&self) -> Result<Vec<TaskId>> {
        let mut ids = Vec::new();
        let refs = self.repo.references_glob(&format!("{REF_PREFIX}*"))?;
        for r in refs {
            let r = r?;
            if let Some(name) = r.name() {
                if let Some(id) = name.strip_prefix(REF_PREFIX) {
                    if id == CONFIG_ID {
                        continue; // reserved config ref, not a task
                    }
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// The task's current ref target. Errors if the task doesn't exist — see
    /// `find_tip` for the non-erroring version used by merge reconciliation.
    pub fn tip(&self, id: &str) -> Result<Oid> {
        self.repo
            .refname_to_id(&format!("{REF_PREFIX}{id}"))
            .with_context(|| format!("task {id} not found"))
    }

    /// Like `tip`, but `None` (not an error) when the task doesn't exist locally.
    pub fn find_tip(&self, id: &str) -> Result<Option<Oid>> {
        match self.repo.refname_to_id(&format!("{REF_PREFIX}{id}")) {
            Ok(oid) => Ok(Some(oid)),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn is_ancestor(&self, ancestor: Oid, descendant: Oid) -> Result<bool> {
        Ok(self.repo.graph_descendant_of(descendant, ancestor)?)
    }

    /// Moves (or creates, if `id` is new locally) the task's ref straight to `tip` —
    /// valid when `tip` is already known to be a descendant of the current target,
    /// or there is no current target yet.
    pub fn set_ref(&self, id: &str, tip: Oid, force: bool) -> Result<()> {
        let ref_name = format!("{REF_PREFIX}{id}");
        self.repo
            .reference(&ref_name, tip, force, "git-task: update")
            .with_context(|| format!("updating ref {ref_name}"))?;
        Ok(())
    }

    /// Hard delete: removes the local `refs/tasks/<id>` ref outright. Unlike `append`ing
    /// `Operation::DeleteTask`, this writes no commit and carries no provenance, so it does
    /// not sync — `push` can only push refs that still exist locally, and a later
    /// `pull`/`clone` from a peer that still holds the task recreates it fresh (see
    /// `merge::reconcile`'s `New` case). Local-only, one-way in the sense that undoing it
    /// means re-fetching from somewhere that still has it, not anything this store tracks.
    pub fn drop(&self, id: &TaskId) -> Result<()> {
        let ref_name = format!("{REF_PREFIX}{id}");
        let mut r = self
            .repo
            .find_reference(&ref_name)
            .with_context(|| format!("task {id} not found"))?;
        r.delete().with_context(|| format!("deleting ref {ref_name}"))
    }

    /// Reconciles two divergent tips with a real two-parent merge commit carrying no
    /// ops of its own — `load`'s DAG walk derives the same task state regardless of
    /// which side performs the merge, since it only depends on the (shared) set of
    /// reachable commits, not on the merge commit's own identity.
    pub fn merge(&self, id: &TaskId, local_tip: Oid, remote_tip: Oid, author: &Actor) -> Result<()> {
        let commit_oid =
            self.write_commit(OPS_BLOB_NAME, None, &[local_tip, remote_tip], author, now(), "merge")?;
        self.set_ref(id, commit_oid, true)
    }

    /// Writes one commit whose tree holds `blob` (if any) under `blob_name`, parented on
    /// `parents`, without moving any ref (`commit(None, ...)`) — the caller updates the ref.
    /// Generic over `blob_name`/`blob` so the task op-chain (`ops.json`) and the config
    /// op-chain (`config-ops.json`) share the exact same commit-writing path. A `None` blob
    /// yields an empty tree — used for two-parent merge commits, which carry no ops of their own.
    fn write_commit(
        &self,
        blob_name: &str,
        blob: Option<&[u8]>,
        parents: &[Oid],
        author: &Actor,
        ts: i64,
        message: &str,
    ) -> Result<Oid> {
        let mut builder = self.repo.treebuilder(None).context("creating tree builder")?;
        if let Some(bytes) = blob {
            let blob_oid = self.repo.blob(bytes).context("writing blob")?;
            builder
                .insert(blob_name, blob_oid, BLOB_FILEMODE)
                .with_context(|| format!("inserting {blob_name} into tree"))?;
        }
        let tree_oid = builder.write().context("writing tree")?;
        let tree = self.repo.find_tree(tree_oid)?;

        let sig = Signature::new(&author.name, &author.email, &Time::new(ts, 0))
            .context("building commit signature (check git config user.name/user.email)")?;

        let parent_commits = parents
            .iter()
            .map(|oid| self.repo.find_commit(*oid))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();

        self.repo
            .commit(None, &sig, &sig, message, &tree, &parent_refs)
            .context("writing commit")
    }
}

fn envelope_all(author: &Actor, ops: Vec<Operation>) -> Vec<OpEnvelope> {
    let ts = now();
    ops.into_iter()
        .map(|op| OpEnvelope {
            author: author.clone(),
            timestamp: ts,
            op,
        })
        .collect()
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn op_summary(envelopes: &[OpEnvelope]) -> String {
    envelopes.iter().map(|e| op_tag(&e.op)).collect::<Vec<_>>().join(", ")
}

fn op_tag(op: &Operation) -> &'static str {
    match op {
        Operation::CreateTask { .. } => "CreateTask",
        Operation::SetTitle { .. } => "SetTitle",
        Operation::SetDescription { .. } => "SetDescription",
        Operation::SetKind { .. } => "SetKind",
        Operation::SetStatus { .. } => "SetStatus",
        Operation::SetPriority { .. } => "SetPriority",
        Operation::SetAssignee { .. } => "SetAssignee",
        Operation::AddLabel { .. } => "AddLabel",
        Operation::RemoveLabel { .. } => "RemoveLabel",
        Operation::AddComment { .. } => "AddComment",
        Operation::EditComment { .. } => "EditComment",
        Operation::SetDueDate { .. } => "SetDueDate",
        Operation::SetParent { .. } => "SetParent",
        Operation::ClearParent => "ClearParent",
        Operation::SetMilestone { .. } => "SetMilestone",
        Operation::AddLink { .. } => "AddLink",
        Operation::RemoveLink { .. } => "RemoveLink",
        Operation::DeleteTask => "DeleteTask",
    }
}
