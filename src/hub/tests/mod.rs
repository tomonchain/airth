//! The hub's tests, end to end with a scripted model; shared fixtures here.

use super::delivery::TEST_GITHUB;
use super::objectives::PUSH_OBJECTIVE;
use super::*;
use crate::agent::NoObserver;
use crate::home::NewRealm;
use crate::isolation::Isolation;
use crate::models::{ModelRequest, ModelResponse, StopReason};
use crate::permissions::{Answer, PermissionRequest, Rule};
use serde_json::json;
use std::sync::Mutex;

mod changes;
mod models;
mod objectives;
mod runs;
mod skills;

#[derive(Default)]
struct Scripted {
    responses: Mutex<Vec<ModelResponse>>,
    seen: Arc<Mutex<Vec<Vec<Message>>>>,
    systems: Arc<Mutex<Vec<String>>>,
}

impl Model for Scripted {
    fn name(&self) -> String {
        "scripted".into()
    }
    fn complete(&self, req: &ModelRequest) -> Result<ModelResponse> {
        self.seen.lock().unwrap().push(req.messages.to_vec());
        self.systems
            .lock()
            .unwrap()
            .push(format!("{}\n\n{}", req.system, req.context));
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            // Out of script: behave like a provider outage.
            anyhow::bail!("connection reset by peer");
        }
        Ok(responses.remove(0))
    }
}

struct Fixed(bool);

impl Approver for Fixed {
    fn ask(&mut self, _: &PermissionRequest) -> Result<Answer> {
        Ok(if self.0 {
            Answer::Yes
        } else {
            Answer::No("not in tests".into())
        })
    }
}

fn resp(content: Vec<ContentBlock>, stop_reason: StopReason) -> ModelResponse {
    ModelResponse {
        content,
        stop_reason,
        model: "scripted".into(),
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
    }
}

fn call(id: &str, name: &str, input: serde_json::Value) -> ModelResponse {
    resp(
        vec![ContentBlock::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }],
        StopReason::ToolUse,
    )
}

fn done(text: &str) -> ModelResponse {
    resp(
        vec![ContentBlock::Text { text: text.into() }],
        StopReason::EndTurn,
    )
}

fn config(max_iterations: u32) -> AgentConfig {
    AgentConfig::new(max_iterations)
}

fn hooks(observer: &mut NoObserver, approve: bool) -> RunHooks<'_> {
    RunHooks {
        approver: Box::new(Fixed(approve)),
        observer,
        cancel: CancelToken::default(),
    }
}

/// A home with one realm rooted at a fresh directory.
struct Fixture {
    _home: tempfile::TempDir,
    root: tempfile::TempDir,
    home: Home,
    realm: Realm,
}

fn fixture() -> Fixture {
    let home_dir = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let home = Home::at(home_dir.path()).unwrap();
    let realm = home
        .create_realm(NewRealm {
            name: "work".into(),
            description: "the day job".into(),
            root: Some(root.path().to_path_buf()),
            isolation: None,
        })
        .unwrap();
    Fixture {
        _home: home_dir,
        root,
        home,
        realm,
    }
}

type Seen = Arc<Mutex<Vec<Vec<Message>>>>;

fn make_hub(home: &Home, responses: Vec<ModelResponse>) -> (Hub, Seen, Arc<Mutex<Vec<String>>>) {
    let model = Scripted {
        responses: Mutex::new(responses),
        ..Scripted::default()
    };
    let (seen, systems) = (model.seen.clone(), model.systems.clone());
    (
        Hub::new(home.clone()).with_model(Box::new(model)),
        seen,
        systems,
    )
}
