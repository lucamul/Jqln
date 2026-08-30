//! Driving the assistant panel: the toggle, key handling, sending a turn,
//! draining the stream, and applying comment proposals.

use super::*;
use crate::assistant::{comments, context, cost, keyring, provider, Role};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

impl App {
    /// F9: open the pane (and focus it), or close it.
    pub(crate) fn assistant_toggle(&mut self) {
        if !self.ai_available {
            self.status =
                "AI assistant is off. Relaunch with: jqln --with-ai-assistant".to_string();
            return;
        }
        if self.view != View::Editor {
            self.view = View::Editor;
        }
        let a = &mut self.assistant;
        if a.open && a.focused {
            a.open = false;
            a.focused = false;
            self.focus = Focus::Editor;
        } else {
            a.open = true;
            a.focused = true;
            if a.turns.is_empty() {
                let provider = self.project.assistant.provider.clone();
                self.assistant_greeting(&provider);
            }
        }
    }

    fn assistant_greeting(&mut self, provider: &str) {
        let key = keyring::resolve(provider);
        let a = &mut self.assistant;
        match key {
            Ok(_) => a.say(
                Role::Local,
                format!(
                    "{provider} · {} · context: {}\nType a message, or /help for commands.",
                    a.model,
                    a.scope.label()
                ),
            ),
            Err(e) => a.say(Role::Local, e),
        }
    }

    /// Keys while the assistant pane has focus.
    pub(crate) fn assistant_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                self.assistant.focused = false;
                self.focus = Focus::Editor;
            }
            KeyCode::Char('c') if ctrl => {
                if self.assistant.busy {
                    self.assistant.cancel.store(true, Ordering::Relaxed);
                    self.assistant.say(Role::Local, "cancelled.");
                    self.assistant.busy = false;
                    self.assistant.rx = None;
                }
            }
            KeyCode::PageUp => self.assistant.scroll_back = self.assistant.scroll_back.saturating_add(6),
            KeyCode::PageDown => {
                self.assistant.scroll_back = self.assistant.scroll_back.saturating_sub(6)
            }
            KeyCode::Enter if !alt && !key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.assistant_submit();
            }
            _ => {
                self.assistant.input.input(key);
            }
        }
    }

    /// Enter pressed: run a `/command` or send the line as a message.
    fn assistant_submit(&mut self) {
        let text = self.assistant.input.lines().join("\n").trim().to_string();
        if text.is_empty() || self.assistant.busy {
            return;
        }
        self.assistant.clear_input();

        if let Some(rest) = text.strip_prefix('/') {
            self.assistant_command(rest.trim());
            return;
        }
        self.assistant_send(text);
    }

    fn assistant_command(&mut self, cmd: &str) {
        let (name, arg) = cmd.split_once(char::is_whitespace).unwrap_or((cmd, ""));
        let arg = arg.trim();
        match name {
            "help" | "?" => self.assistant.say(
                Role::Local,
                "/key                      paste an API key (saved to ~/.config/jqln/config.toml)\n\
                 /provider anthropic|openai\n\
                 /model <id>               set and remember the model\n\
                 /context <scope>          document | document+outline | chapter | manuscript | selection (no arg cycles)\n\
                 /comments on|off          let the assistant propose inline comments\n\
                 /apply [N] | /apply skip  insert proposed comment(s)\n\
                 /clear                    wipe this conversation\n\
                 /yes  /no                 answer the send-to-provider prompt",
            ),
            "context" | "ctx" => {
                let s = if arg.is_empty() {
                    Some(self.assistant.scope.next())
                } else {
                    context::Scope::parse(arg)
                };
                match s {
                    Some(s) => {
                        self.assistant.scope = s;
                        self.project.assistant.default_context = s.key().to_string();
                        self.dirty = true;
                        self.assistant.say(Role::Local, format!("context: {}", s.label()));
                    }
                    None => self.assistant.say(
                        Role::Local,
                        "usage: /context <document|document+outline|chapter|manuscript|selection>",
                    ),
                }
            }
            "model" => {
                if arg.is_empty() {
                    let sugg = cost_suggestions(&self.project.assistant.provider).join(", ");
                    self.assistant.say(Role::Local, format!("current: {}\nsuggestions: {sugg}", self.assistant.model));
                } else {
                    self.assistant.model = arg.to_string();
                    self.project.assistant.model = arg.to_string();
                    self.dirty = true;
                    self.assistant.say(Role::Local, format!("model: {arg}"));
                }
            }
            "comments" => match arg {
                "on" | "true" => {
                    self.project.assistant.allow_comments = true;
                    self.dirty = true;
                    self.assistant.say(Role::Local, "inline comments: on");
                }
                "off" | "false" => {
                    self.project.assistant.allow_comments = false;
                    self.dirty = true;
                    self.assistant.say(Role::Local, "inline comments: off");
                }
                _ => self.assistant.say(
                    Role::Local,
                    format!("inline comments: {}", if self.project.assistant.allow_comments { "on" } else { "off" }),
                ),
            },
            "provider" => match arg {
                "anthropic" | "openai" => {
                    self.project.assistant.provider = arg.to_string();
                    self.dirty = true;
                    let p = arg.to_string();
                    self.assistant.say(Role::Local, format!("provider: {p}"));
                }
                _ => self.assistant.say(Role::Local, "usage: /provider anthropic|openai"),
            },
            "key" => self.open_key_prompt(),
            "clear" => {
                self.assistant.reset();
                let p = self.project.assistant.provider.clone();
                self.assistant_greeting(&p);
            }
            "apply" => self.assistant_apply(arg),
            "yes" | "y" => {
                if let Some(msg) = self.assistant.pending.take() {
                    self.assistant.confirmed = true;
                    self.assistant_send(msg);
                } else {
                    self.assistant.say(Role::Local, "nothing to confirm.");
                }
            }
            "no" | "n" => {
                self.assistant.pending = None;
                self.assistant.say(Role::Local, "cancelled.");
            }
            other => self.assistant.say(Role::Local, format!("unknown command /{other} — try /help")),
        }
    }

    /// Open the paste-your-key popup, remembering `pending` so the message that
    /// triggered it can go through once the key is saved.
    pub(crate) fn open_key_prompt(&mut self) {
        self.input = single_line("");
        self.modal = Modal::AssistantKey;
    }

    /// Enter / Esc in the key popup.
    pub(crate) fn assistant_key_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
                self.assistant.pending = None;
                self.assistant.say(Role::Local, "cancelled — no key saved.");
            }
            KeyCode::Enter => {
                let pasted = self.input.lines().concat().trim().to_string();
                self.modal = Modal::None;
                if pasted.is_empty() {
                    self.assistant.say(Role::Local, "nothing pasted.");
                    return;
                }
                let provider = self.project.assistant.provider.clone();
                match keyring::save(&provider, &pasted) {
                    Ok(path) => {
                        self.assistant.confirmed = true; // the popup stated what is sent
                        self.assistant.say(Role::Local, format!("key saved to {}", path.display()));
                        if let Some(msg) = self.assistant.pending.take() {
                            self.assistant_send(msg);
                        }
                    }
                    Err(e) => self.assistant.say(Role::Local, format!("could not save key: {e}")),
                }
            }
            _ => {
                self.input.input(key);
            }
        }
    }

    fn assistant_send(&mut self, message: String) {
        let provider = self.project.assistant.provider.clone();
        let api_key = match keyring::resolve(&provider) {
            Ok(k) => k,
            Err(_) => {
                // No key yet: stash the message and ask for one.
                self.assistant.pending = Some(message);
                self.assistant.say(
                    Role::Local,
                    format!("No {provider} key — opening the paste box (Esc to cancel)."),
                );
                self.open_key_prompt();
                return;
            }
        };
        if !self.assistant.confirmed {
            self.assistant.pending = Some(message);
            self.assistant.say(
                Role::Local,
                format!("This sends your text to {provider}. /yes to continue, /no to cancel."),
            );
            return;
        }

        // Freshen bodies from open editors, then build the context.
        self.flush();
        let current = self.editor_doc();
        let selection = self.editor_selection_text();
        let built = context::build(
            &mut self.project,
            current.as_ref(),
            selection.as_deref(),
            self.assistant.scope,
        );

        let mut system = context::SYSTEM.to_string();
        if self.project.assistant.allow_comments {
            system.push_str(context::SYSTEM_COMMENTS);
        }

        // Conversation so far (only real turns), then this message with context.
        let mut msgs: Vec<provider::Msg> = self
            .assistant
            .turns
            .iter()
            .filter_map(|t| match t.role {
                Role::User => Some(provider::Msg { role: "user", text: t.text.clone() }),
                Role::Assistant => Some(provider::Msg { role: "assistant", text: t.text.clone() }),
                Role::Local => None,
            })
            .collect();
        msgs.push(provider::Msg {
            role: "user",
            text: format!("{}\n\n--- context ({}) ---\n{}", message, built.summary, built.text),
        });

        self.assistant.say(Role::Local, format!("sending: {}", built.summary));
        self.assistant.say(Role::User, message);
        self.assistant.say(Role::Assistant, String::new());
        self.assistant.busy = true;
        self.assistant.proposals.clear();

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.assistant.rx = Some(rx);
        self.assistant.cancel = cancel.clone();

        let req = provider::Request {
            provider,
            api_key,
            model: self.assistant.model.clone(),
            system,
            messages: msgs,
            base_url: std::env::var("JQLN_ASSISTANT_BASE_URL").ok(),
        };
        std::thread::spawn(move || provider::run(req, &tx, &cancel));
    }

    /// Drain streamed tokens. Called every loop tick.
    pub(crate) fn assistant_poll(&mut self) {
        let Some(rx) = self.assistant.rx.take() else { return };
        let mut done = false;
        loop {
            match rx.try_recv() {
                Ok(provider::Event::Token(t)) => {
                    if let Some(turn) = self.assistant.streaming_turn() {
                        turn.text.push_str(&t);
                    }
                }
                Ok(provider::Event::Done { input, output }) => {
                    self.assistant.tokens_in += input;
                    self.assistant.tokens_out += output;
                    done = true;
                    break;
                }
                Ok(provider::Event::Error(e)) => {
                    self.assistant.say(Role::Local, format!("error: {e}"));
                    done = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }

        if done {
            self.assistant.busy = false;
            self.finish_assistant_reply();
        } else {
            self.assistant.rx = Some(rx);
        }
    }

    fn finish_assistant_reply(&mut self) {
        let text = match self.assistant.streaming_turn() {
            Some(t) if !t.text.is_empty() => t.text.clone(),
            Some(t) => {
                t.text.push_str("(no reply)");
                return;
            }
            None => return,
        };
        if !self.project.assistant.allow_comments {
            return;
        }
        let (proposals, cleaned) = comments::extract(&text);
        if let Some(t) = self.assistant.streaming_turn() {
            t.text = cleaned;
        }
        if !proposals.is_empty() {
            let n = proposals.len();
            self.assistant.proposals = proposals;
            self.assistant.say(
                Role::Local,
                format!(
                    "{n} comment{} proposed — /apply to insert, /apply skip to drop.",
                    if n == 1 { "" } else { "s" }
                ),
            );
        }
    }

    fn assistant_apply(&mut self, arg: &str) {
        if self.assistant.proposals.is_empty() {
            self.assistant.say(Role::Local, "no proposals to apply.");
            return;
        }
        if arg == "skip" || arg == "drop" || arg == "no" {
            self.assistant.proposals.clear();
            self.assistant.say(Role::Local, "proposals dropped.");
            return;
        }
        let Some(id) = self.editor_doc() else {
            self.assistant.say(Role::Local, "open a document first.");
            return;
        };
        self.ensure_editor(&id);

        let picked: Vec<comments::Proposal> = match arg.parse::<usize>() {
            Ok(n) if n >= 1 && n <= self.assistant.proposals.len() => {
                vec![self.assistant.proposals[n - 1].clone()]
            }
            Ok(_) => {
                self.assistant.say(Role::Local, "no proposal with that number.");
                return;
            }
            Err(_) => self.assistant.proposals.clone(),
        };

        let prefix = self.project.assistant.comment_prefix.clone();
        let body = self.editors.get(&id).map(|ta| ta.lines().join("\n")).unwrap_or_default();
        let (new_body, report) = comments::apply(&body, &prefix, &picked);

        if new_body != body
            && let Some(ta) = self.editors.get_mut(&id)
        {
            // One replace = one undo step.
            ta.select_all();
            ta.insert_str(&new_body);
            self.dirty = true;
        }
        self.restyle(&id);
        self.assistant.proposals.clear();
        self.assistant.say(Role::Local, report.join("\n"));
    }

    /// The selected text in the open editor, if any.
    fn editor_selection_text(&self) -> Option<String> {
        let id = self.editor_doc()?;
        let ta = self.editors.get(&id)?;
        let ((sr, sc), (er, ec)) = ta.selection_range()?;
        let lines = ta.lines();
        if sr == er {
            let l = &lines[sr];
            Some(l[crate::markup::byte_index(l, sc)..crate::markup::byte_index(l, ec)].to_string())
        } else {
            let mut parts = vec![lines[sr][crate::markup::byte_index(&lines[sr], sc)..].to_string()];
            parts.extend(lines[sr + 1..er].iter().cloned());
            parts.push(lines[er][..crate::markup::byte_index(&lines[er], ec)].to_string());
            Some(parts.join("\n"))
        }
    }
}

fn cost_suggestions(provider: &str) -> &'static [&'static str] {
    match provider {
        "openai" => &["gpt-5", "gpt-5-mini", "gpt-4.1", "gpt-4o", "gpt-4o-mini"],
        _ => &["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5-20251001"],
    }
}

/// Header string for the pane: context · document · model · session spend.
/// Cheap enough to build every frame.
pub(crate) fn header(app: &App) -> String {
    let a = &app.assistant;
    let total = a.tokens_in + a.tokens_out;
    let spend = match cost::estimate(&a.model, a.tokens_in, a.tokens_out) {
        Some(usd) => format!("{} ≈ ${usd:.2}", cost::tokens(total)),
        None => cost::tokens(total),
    };
    let doc = app
        .editor_doc()
        .and_then(|id| app.project.nodes.get(&id).map(|n| n.title.clone()))
        .unwrap_or_else(|| "—".to_string());
    format!("{} · {doc} · {} · {spend}", a.scope.label(), a.model)
}
