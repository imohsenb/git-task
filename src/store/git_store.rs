use anyhow::{Context, Result};
use git2::{Oid, Repository, Signature, Time};

use crate::actor::Actor;
use crate::domain::fold::fold;
use crate::domain::id::{short, TaskId};
use crate::domain::op::{Operation, OpEnvelope};
use crate::domain::task::Task;
use crate::output::ClassifiedError;

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

    /// Appends `ops` onto the tip, guarding against a racing second writer (another terminal
    /// invocation, or — within this same process — `automation::engine` appending its own
    /// cascaded ops right after this call returns) via compare-and-swap: `set_ref_cas` only
    /// moves the ref if it still points at the tip this call started from, so a race fails
    /// loudly (retried once against the fresh tip, then surfaced as a `Conflict`) instead of one
    /// side's commit silently winning and orphaning the other's — unreachable, unmerged, absent
    /// from `load`, with no error to say so. A caller-side mutex can't protect against this since
    /// the race is between separate OS processes, not threads within one.
    pub fn append(&self, id: &TaskId, author: &Actor, ops: Vec<Operation>) -> Result<()> {
        let envelopes = envelope_all(author, ops);
        let message = op_summary(&envelopes);
        let bytes = serde_json::to_vec_pretty(&envelopes).context("serializing ops")?;
        let ts = envelopes.first().map(|e| e.timestamp).unwrap_or_else(now);

        let mut tip = self.tip(id)?;
        for attempt in 0..2 {
            let commit_oid = self.write_commit(OPS_BLOB_NAME, Some(&bytes), &[tip], author, ts, &message)?;
            match self.set_ref_cas(id, commit_oid, tip) {
                Ok(()) => return Ok(()),
                Err(_) if attempt == 0 => tip = self.tip(id)?,
                Err(_) => return Err(anyhow::Error::new(append_conflict(id))),
            }
        }
        unreachable!("loop always returns within its two attempts")
    }

    /// Create-or-append a single-blob commit on `refs/tasks/<id>`, carrying `blob` under
    /// `blob_name`. Used for the reserved config chain (`CONFIG_ID`); tasks go through
    /// `create`/`append`, which additionally build op-summary commit messages. The append arm
    /// races the same way `append` does (and is guarded the same way); the create arm already
    /// gets its own compare-and-swap for free from `reference`'s `force: false`, which fails
    /// outright if the ref now exists rather than silently overwriting it.
    pub fn append_chain(
        &self,
        id: &str,
        author: &Actor,
        blob_name: &str,
        blob: &[u8],
        ts: i64,
        message: &str,
    ) -> Result<()> {
        let Some(mut tip) = self.find_tip(id)? else {
            let oid = self.write_commit(blob_name, Some(blob), &[], author, ts, message)?;
            return self.set_ref(id, oid, false);
        };

        for attempt in 0..2 {
            let oid = self.write_commit(blob_name, Some(blob), &[tip], author, ts, message)?;
            match self.set_ref_cas(id, oid, tip) {
                Ok(()) => return Ok(()),
                Err(_) if attempt == 0 => tip = self.tip(id)?,
                Err(_) => return Err(anyhow::Error::new(append_conflict(id))),
            }
        }
        unreachable!("loop always returns within its two attempts")
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
            0 => Err(anyhow::Error::new(ClassifiedError::NotFound {
                message: format!("no task matching '{prefix}'"),
                query: prefix.to_string(),
                entity: "task".to_string(),
            })),
            1 => Ok(matches.remove(0)),
            _ => {
                matches.sort();
                let list = matches.iter().map(|m| short(m)).collect::<Vec<_>>().join(", ");
                Err(anyhow::Error::new(ClassifiedError::AmbiguousId {
                    message: format!("'{prefix}' is ambiguous, matches: {list}"),
                    query: prefix.to_string(),
                    matches,
                }))
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

    /// Compare-and-swap: moves the ref to `new_tip` only if it still points at `expected` —
    /// libgit2's own atomic guard (`git_reference_create_matching`), not a check-then-set this
    /// crate does itself. Fails (rather than overwriting) if another writer moved the ref first.
    /// `pub(crate)` (not `pub`) — `store::merge::reconcile` also drives this directly to guard its
    /// own fast-forward/merge writes, but it's not part of the store's public API.
    pub(crate) fn set_ref_cas(&self, id: &str, new_tip: Oid, expected: Oid) -> std::result::Result<(), git2::Error> {
        let ref_name = format!("{REF_PREFIX}{id}");
        self.repo.reference_matching(&ref_name, new_tip, true, expected, "git-task: update")?;
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
    /// reachable commits, not on the merge commit's own identity. Moves the ref via
    /// compare-and-swap against `local_tip` rather than a blind force-update, so a second
    /// `pull` racing this one (or a local `append` landing between `reconcile`'s read of
    /// `local_tip` and this write) can't have its commit silently orphaned by this one
    /// overwriting the ref out from under it — see `merge::reconcile`'s retry loop, which
    /// is what actually handles the resulting error.
    pub fn merge(&self, id: &TaskId, local_tip: Oid, remote_tip: Oid, author: &Actor) -> Result<()> {
        let commit_oid =
            self.write_commit(OPS_BLOB_NAME, None, &[local_tip, remote_tip], author, now(), "merge")?;
        self.set_ref_cas(id, commit_oid, local_tip)?;
        Ok(())
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

fn append_conflict(id: &str) -> ClassifiedError {
    ClassifiedError::Conflict {
        message: format!(
            "task {id} was updated concurrently by another writer — retried once and still lost the race; run the command again"
        ),
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
    envelopes.iter().map(|e| e.op.tag()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::op::TaskKind;

    fn actor() -> Actor {
        Actor { name: "Test".into(), email: "test@example.com".into() }
    }

    fn temp_store() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Repository::init(dir.path()).expect("init repo");
        (dir, repo)
    }

    #[test]
    fn set_ref_cas_rejects_a_stale_expected_tip() {
        let (_dir, repo) = temp_store();
        let store = Store::new(&repo);
        let id = store.create(&actor(), vec![Operation::CreateTask {
            title: "T".into(),
            kind: TaskKind::Task,
            description: "d".into(),
        }]).unwrap();
        let tip_a = store.tip(&id).unwrap();

        // A second writer appends first, moving the ref past `tip_a`.
        store.append(&id, &actor(), vec![Operation::SetStatus { status: "doing".into() }]).unwrap();
        let tip_b = store.tip(&id).unwrap();
        assert_ne!(tip_a, tip_b, "the second append should have moved the ref");

        // A write still holding the now-stale `tip_a` as its expected current value must be
        // rejected, not silently overwrite `tip_b` — this is the exact race `append`'s
        // compare-and-swap guards against.
        let bogus_commit = tip_b; // any existing oid works as the "new" value for this check
        assert!(
            store.set_ref_cas(&id, bogus_commit, tip_a).is_err(),
            "set_ref_cas must reject a stale expected tip instead of overwriting"
        );

        // The ref must be untouched by the rejected attempt.
        assert_eq!(store.tip(&id).unwrap(), tip_b);
    }

    #[test]
    fn set_ref_cas_accepts_a_current_expected_tip() {
        let (_dir, repo) = temp_store();
        let store = Store::new(&repo);
        let id = store.create(&actor(), vec![Operation::CreateTask {
            title: "T".into(),
            kind: TaskKind::Task,
            description: "d".into(),
        }]).unwrap();
        let tip = store.tip(&id).unwrap();

        let new_commit = store
            .write_commit(OPS_BLOB_NAME, None, &[tip], &actor(), now(), "test")
            .unwrap();
        assert!(store.set_ref_cas(&id, new_commit, tip).is_ok());
        assert_eq!(store.tip(&id).unwrap(), new_commit);
    }

    /// `merge` used to move the ref with a blind force-update; a second writer (a racing `pull`,
    /// or a local `append`) landing between `reconcile` reading `local_tip` and this call would
    /// have been silently overwritten. It now goes through the same CAS `append` uses, so a stale
    /// `local_tip` is rejected instead of clobbering whatever the other writer just committed.
    #[test]
    fn merge_rejects_a_stale_local_tip_instead_of_overwriting() {
        let (_dir, repo) = temp_store();
        let store = Store::new(&repo);
        let id = store.create(&actor(), vec![Operation::CreateTask {
            title: "T".into(),
            kind: TaskKind::Task,
            description: "d".into(),
        }]).unwrap();
        let stale_local_tip = store.tip(&id).unwrap();

        // A second writer appends first, moving the ref past `stale_local_tip`.
        store.append(&id, &actor(), vec![Operation::SetStatus { status: "doing".into() }]).unwrap();
        let real_tip = store.tip(&id).unwrap();
        assert_ne!(stale_local_tip, real_tip);

        // A merge built against the now-stale tip must be rejected, not overwrite `real_tip`.
        assert!(store.merge(&id, stale_local_tip, real_tip, &actor()).is_err());
        assert_eq!(store.tip(&id).unwrap(), real_tip, "the racing writer's commit must survive");
    }

    /// Not a race reproduction (that needs true concurrency to force append's *own* first
    /// attempt to collide) — this instead checks the retry path's plumbing end to end: a repo
    /// that has already moved since the caller last read it still lets a fresh `append` call
    /// succeed (it re-reads the tip itself), and folds correctly afterward.
    #[test]
    fn append_succeeds_against_a_tip_that_moved_since_it_was_last_observed() {
        let (_dir, repo) = temp_store();
        let store = Store::new(&repo);
        let id = store.create(&actor(), vec![Operation::CreateTask {
            title: "T".into(),
            kind: TaskKind::Task,
            description: "d".into(),
        }]).unwrap();

        let _stale_tip = store.tip(&id).unwrap();
        store.append(&id, &actor(), vec![Operation::SetPriority { priority: crate::domain::op::Priority::High }]).unwrap();
        store.append(&id, &actor(), vec![Operation::SetStatus { status: "doing".into() }]).unwrap();

        let task = store.load(&id).unwrap();
        assert_eq!(task.status, "doing");
        assert!(task.priority.is_some());
    }
}
