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
/// two-parent merge commit (see `merge`), so `load` walks all reachable commits
/// (deduped) rather than assuming a single parent per commit.
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

        let mut revwalk = self.repo.revwalk()?;
        revwalk.push(tip)?;

        // (timestamp, commit oid, index within that commit's op batch) — a total order
        // that's identical no matter which side of a merge computes it, since it only
        // depends on the (content-addressed, thus shared) set of reachable commits.
        let mut ordered: Vec<(i64, String, usize, OpEnvelope)> = Vec::new();
        for oid in revwalk {
            let oid = oid?;
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
            let oid_hex = oid.to_string();
            for (idx, env) in batch.into_iter().enumerate() {
                ordered.push((env.timestamp, oid_hex.clone(), idx, env));
            }
        }
        ordered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)));

        let ops: Vec<OpEnvelope> = ordered.into_iter().map(|(_, _, _, env)| env).collect();
        fold(id, &ops)
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
