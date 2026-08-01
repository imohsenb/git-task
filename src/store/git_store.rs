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
        let commit_oid = self.write_commit(&envelopes, &[], author, "create task")?;
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
        let commit_oid = self.write_commit(&envelopes, &[tip], author, &message)?;
        self.set_ref(id, commit_oid, true)
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
        let tip = self.tip(id)?;
        let order = self.topological_order(tip)?;

        let mut ops: Vec<OpEnvelope> = Vec::new();
        for oid in order {
            let commit = self.repo.find_commit(oid)?;
            let tree = commit.tree()?;
            let Some(entry) = tree.get_name(OPS_BLOB_NAME) else {
                continue; // merge commits carry no ops of their own
            };
            let blob = entry
                .to_object(self.repo)?
                .into_blob()
                .map_err(|_| anyhow::anyhow!("{OPS_BLOB_NAME} is not a blob in commit {oid}"))?;
            let text = std::str::from_utf8(blob.content())
                .with_context(|| format!("{OPS_BLOB_NAME} in commit {oid} is not valid utf-8"))?;
            let batch: Vec<OpEnvelope> = serde_json::from_str(text)
                .with_context(|| format!("parsing {OPS_BLOB_NAME} in commit {oid}"))?;
            ops.extend(batch);
        }

        fold(id, &ops)
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
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// The task's current ref target. Errors if the task doesn't exist — see
    /// `find_tip` for the non-erroring version used by merge reconciliation.
    pub fn tip(&self, id: &TaskId) -> Result<Oid> {
        self.repo
            .refname_to_id(&format!("{REF_PREFIX}{id}"))
            .with_context(|| format!("task {id} not found"))
    }

    /// Like `tip`, but `None` (not an error) when the task doesn't exist locally.
    pub fn find_tip(&self, id: &TaskId) -> Result<Option<Oid>> {
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
    pub fn set_ref(&self, id: &TaskId, tip: Oid, force: bool) -> Result<()> {
        let ref_name = format!("{REF_PREFIX}{id}");
        self.repo
            .reference(&ref_name, tip, force, "git-task: update")
            .with_context(|| format!("updating ref {ref_name}"))?;
        Ok(())
    }

    /// Reconciles two divergent tips with a real two-parent merge commit carrying no
    /// ops of its own — `load`'s DAG walk derives the same task state regardless of
    /// which side performs the merge, since it only depends on the (shared) set of
    /// reachable commits, not on the merge commit's own identity.
    pub fn merge(&self, id: &TaskId, local_tip: Oid, remote_tip: Oid, author: &Actor) -> Result<()> {
        let commit_oid = self.write_commit(&[], &[local_tip, remote_tip], author, "merge")?;
        self.set_ref(id, commit_oid, true)
    }

    fn write_commit(
        &self,
        envelopes: &[OpEnvelope],
        parents: &[Oid],
        author: &Actor,
        message: &str,
    ) -> Result<Oid> {
        let mut builder = self.repo.treebuilder(None).context("creating tree builder")?;
        if !envelopes.is_empty() {
            let json = serde_json::to_vec_pretty(envelopes).context("serializing ops")?;
            let blob_oid = self.repo.blob(&json).context("writing ops blob")?;
            builder
                .insert(OPS_BLOB_NAME, blob_oid, BLOB_FILEMODE)
                .context("inserting ops.json into tree")?;
        }
        let tree_oid = builder.write().context("writing tree")?;
        let tree = self.repo.find_tree(tree_oid)?;

        let ts = envelopes.first().map(|e| e.timestamp).unwrap_or_else(now);
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
    }
}
