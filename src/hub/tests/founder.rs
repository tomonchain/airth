//! Decision 56: an action that commits is the founder's own call, each
//! time, in the runs the founder starts too.

use super::autonomy::{business, post, service};
use super::*;
use crate::business::store::Store;
use crate::runtime::COMMITS_WARNING;
use std::sync::atomic::Ordering;

/// The founder as an approver: answers in turn (then yes), says in turn
/// whether they watch (then the last), and keeps what they were asked.
struct Founder {
    answers: Vec<Answer>,
    watching: Mutex<Vec<bool>>,
    asked: Arc<Mutex<Vec<PermissionRequest>>>,
}

impl Approver for Founder {
    fn ask(&mut self, request: &PermissionRequest) -> Result<Answer> {
        self.asked.lock().unwrap().push(request.clone());
        Ok(if self.answers.is_empty() {
            Answer::Yes
        } else {
            self.answers.remove(0)
        })
    }

    fn watched(&self) -> bool {
        let mut w = self.watching.lock().unwrap();
        if w.len() > 1 { w.remove(0) } else { w[0] }
    }
}

type Asked = Arc<Mutex<Vec<PermissionRequest>>>;

fn founder(answers: Vec<Answer>, watching: Vec<bool>) -> (Box<Founder>, Asked) {
    let asked = Asked::default();
    let f = Founder {
        answers,
        watching: Mutex::new(watching),
        asked: asked.clone(),
    };
    (Box::new(f), asked)
}

/// A run the founder starts, driven with `approver`.
fn task(f: &Fixture, script: Vec<ModelResponse>, approver: Box<dyn Approver>) -> State {
    let (hub, _, _) = make_hub(&f.home, script);
    let hooks = RunHooks {
        approver,
        observer: &mut NoObserver,
        cancel: CancelToken::default(),
    };
    hub.dispatch(&f.realm, "Publish it", config(10), hooks)
        .unwrap()
}

fn pending(f: &Fixture) -> Vec<crate::business::store::queue::Item> {
    Store::of(&f.realm).unwrap().queue(&["pending"]).unwrap()
}

/// The realm allows the network without asking, and the founder answers
/// "yes for the rest of the run": each publish is still asked, on its
/// own, and a draft (reversible) follows the policy.
#[test]
fn a_watched_run_asks_each_commit_whatever_the_policy_and_the_grants() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    let (approver, asked) = founder(vec![Answer::YesForRun, Answer::YesForRun], vec![true]);
    let state = task(
        &f,
        vec![
            post("1", "posts"),
            post("2", "drafts"),
            post("3", "posts"),
            done("done"),
        ],
        approver,
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2, "each publish, and only they, was asked");
    for request in asked.iter() {
        assert!(request.warnings.iter().any(|w| w == COMMITS_WARNING));
        assert!(
            !request.grantable(),
            "never offered for the rest of the run"
        );
    }
    assert!(state.grants.is_empty(), "no grant was kept");
    assert_eq!(hits.load(Ordering::SeqCst), 3);
    assert!(pending(&f).is_empty());
}

/// Where the policy asks, a grant the founder gave for the connector does
/// not answer for a commit.
#[test]
fn a_grant_for_the_connector_does_not_cover_a_commit() {
    let (base, hits) = service(|_| {});
    let mut f = business(base);
    f.realm.config.policy.network = Rule::Ask;
    f.realm.save().unwrap();
    let (approver, asked) = founder(
        vec![Answer::YesForRun, Answer::No("not now".into())],
        vec![true],
    );
    let state = task(
        &f,
        vec![
            post("1", "drafts"),
            post("2", "posts"),
            post("3", "drafts"),
            done("done"),
        ],
        approver,
    );
    let tools: Vec<bool> = asked
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.warnings.iter().any(|w| w == COMMITS_WARNING))
        .collect();
    assert_eq!(
        tools,
        vec![false, true],
        "the draft once, the publish anyway"
    );
    assert_eq!(state.events[1].status, crate::state::EventStatus::Denied);
    assert_eq!(hits.load(Ordering::SeqCst), 2, "both drafts, no publish");
}

/// Nobody watches (an order, a closed window, any approver that does not
/// say it watches, Telegram included): the run stops at the commit, which
/// waits in Needs you, is never sent by the run, and is done once on
/// approval. An approver that would say yes is not even asked.
#[test]
fn an_unwatched_run_stops_and_the_commit_waits_in_needs_you() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    let both = resp(
        vec![
            ContentBlock::ToolUse {
                id: "1".into(),
                name: "http_request".into(),
                input: json!({"integration": "blog", "method": "POST", "url": "posts"}),
            },
            ContentBlock::ToolUse {
                id: "2".into(),
                name: "http_request".into(),
                input: json!({"integration": "blog", "method": "POST", "url": "drafts"}),
            },
        ],
        StopReason::ToolUse,
    );
    let state = task(&f, vec![both, done("published")], Box::new(Fixed(true)));
    assert_eq!(state.status, Status::Stopped);
    let report = state.report.clone().unwrap();
    assert!(report.contains("Needs you (item 1)"), "{report}");
    assert_eq!(state.events.len(), 1, "the call after it was not run");
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    let items = pending(&f);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].action.as_deref(), Some("blog.publish"));
    assert_eq!(items[0].run.as_deref(), Some(state.id.as_str()));

    let (hub, _, _) = make_hub(&f.home, vec![]);
    let approve = crate::business::store::queue::Answer::Approve;
    hub.answer_item(&f.realm, 1, None, approve, "app", 5)
        .unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 1, "done on approval");
    let again = crate::business::store::queue::Answer::Approve;
    let _ = hub.answer_item(&f.realm, 1, None, again, "app", 5);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "and only once");
}

/// The founder watches, then leaves: every later commit is unwatched, even
/// if they come back.
#[test]
fn once_the_founder_leaves_every_later_commit_is_unwatched() {
    let (base, hits) = service(|_| {});
    let f = business(base);
    let (approver, asked) = founder(vec![], vec![true, false, true]);
    let state = task(
        &f,
        vec![
            post("1", "posts"),
            post("2", "drafts"),
            post("3", "posts"),
            done("done"),
        ],
        approver,
    );
    assert_eq!(asked.lock().unwrap().len(), 1, "only the first publish");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "the first publish, the draft"
    );
    assert_eq!(state.status, Status::Stopped);
    assert_eq!(pending(&f).len(), 1);
}
