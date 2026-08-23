use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use rx4::provider::{Message, Provider, ProviderError, StreamResult};
use serde_json::Value;

use crate::exec::ExecArgs;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelRoles {
    pub default: String,
    pub smol: Option<String>,
    pub slow: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug)]
pub struct ModelRouting {
    pub roles: ModelRoles,
    pub prewalk: bool,
    pub plan_yolo: bool,
    switched: AtomicBool,
}

impl Clone for ModelRouting {
    fn clone(&self) -> Self {
        Self {
            roles: self.roles.clone(),
            prewalk: self.prewalk,
            plan_yolo: self.plan_yolo,
            switched: AtomicBool::new(self.switched.load(Ordering::SeqCst)),
        }
    }
}

impl PartialEq for ModelRouting {
    fn eq(&self, other: &Self) -> bool {
        self.roles == other.roles
            && self.prewalk == other.prewalk
            && self.plan_yolo == other.plan_yolo
            && self.switched() == other.switched()
    }
}

impl Eq for ModelRouting {}

impl Default for ModelRouting {
    fn default() -> Self {
        Self {
            roles: ModelRoles::default(),
            prewalk: false,
            plan_yolo: false,
            switched: AtomicBool::new(false),
        }
    }
}

impl ModelRouting {
    pub fn from_env() -> Self {
        Self::from_parts(
            None,
            env_flag("TK_PREWALK"),
            env_flag("TK_PLAN_YOLO"),
            env_opt("TK_SMOL_MODEL"),
            env_opt("TK_SLOW_MODEL"),
            env_opt("TK_PLAN_MODEL"),
        )
    }

    pub fn from_exec(exec: &ExecArgs) -> Self {
        Self::from_parts(
            exec.model.clone(),
            exec.prewalk || env_flag("TK_PREWALK"),
            exec.plan_yolo || env_flag("TK_PLAN_YOLO"),
            exec.smol.clone().or_else(|| env_opt("TK_SMOL_MODEL")),
            exec.slow.clone().or_else(|| env_opt("TK_SLOW_MODEL")),
            exec.plan_model.clone().or_else(|| env_opt("TK_PLAN_MODEL")),
        )
    }

    fn from_parts(
        default: Option<String>,
        prewalk: bool,
        plan_yolo: bool,
        smol: Option<String>,
        slow: Option<String>,
        plan: Option<String>,
    ) -> Self {
        Self {
            roles: ModelRoles {
                default: default.unwrap_or_default(),
                smol,
                slow,
                plan,
            },
            prewalk: prewalk || plan_yolo,
            plan_yolo,
            switched: AtomicBool::new(false),
        }
    }

    pub fn with_default(mut self, model: impl Into<String>) -> Self {
        self.roles.default = model.into();
        self
    }

    pub fn start_model<'a>(&'a self, configured: &'a str) -> &'a str {
        if self.roles.default.is_empty() {
            if self.plan_yolo {
                if let Some(plan) = self.roles.plan.as_deref() {
                    return plan;
                }
            }
            configured
        } else {
            &self.roles.default
        }
    }

    pub fn implement_model(&self) -> Option<&str> {
        self.roles.smol.as_deref()
    }

    pub fn apply_model(&self, current: &str) -> String {
        if self.switched() {
            self.implement_model().unwrap_or(current).to_string()
        } else {
            current.to_string()
        }
    }

    pub fn sloppy_edit(&self, current: &str) -> bool {
        crate::tools::hashline::sloppy_for_model(&self.apply_model(current))
    }

    pub fn note_mutating_tool(&self) {
        if (self.prewalk || self.plan_yolo) && self.roles.smol.is_some() {
            self.switched.store(true, Ordering::SeqCst);
        }
    }

    pub fn switched(&self) -> bool {
        self.switched.load(Ordering::SeqCst)
    }

    pub fn should_wrap_provider(&self) -> bool {
        (self.prewalk || self.plan_yolo) && self.roles.smol.is_some()
    }
}

pub struct RoutingProvider {
    inner: Arc<dyn Provider>,
    routing: Arc<ModelRouting>,
}

impl RoutingProvider {
    pub fn wrap(inner: Arc<dyn Provider>, routing: Arc<ModelRouting>) -> Arc<dyn Provider> {
        if routing.should_wrap_provider() {
            Arc::new(Self { inner, routing })
        } else {
            inner
        }
    }
}

#[async_trait]
impl Provider for RoutingProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn stream(
        &self,
        messages: &[Message],
        system: &Option<String>,
        model: &str,
        tools: &[Value],
        reasoning_effort: Option<&str>,
    ) -> Result<StreamResult, ProviderError> {
        let model = self.routing.apply_model(model);
        self.inner
            .stream(messages, system, &model, tools, reasoning_effort)
            .await
    }
}

fn env_opt(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref().map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("on") | Some("yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prewalk_switches_one_way_to_smol() {
        let routing = ModelRouting::from_parts(
            Some("gpt-5.6-sol".into()),
            true,
            false,
            Some("gpt-5.6-sol-light".into()),
            None,
            None,
        );
        assert_eq!(routing.start_model("fallback"), "gpt-5.6-sol");
        assert_eq!(routing.apply_model("gpt-5.6-sol"), "gpt-5.6-sol");
        routing.note_mutating_tool();
        assert!(routing.switched());
        assert_eq!(routing.apply_model("gpt-5.6-sol"), "gpt-5.6-sol-light");
        routing.note_mutating_tool();
        assert_eq!(routing.apply_model("gpt-5.6-sol"), "gpt-5.6-sol-light");
    }

    #[test]
    fn plan_yolo_starts_on_plan_role_then_implements_on_smol() {
        let routing = ModelRouting::from_parts(
            None,
            false,
            true,
            Some("gpt-5.6-sol-light".into()),
            None,
            Some("gpt-5.6-sol".into()),
        );
        assert!(routing.prewalk);
        assert_eq!(routing.start_model("configured"), "gpt-5.6-sol");
        routing.note_mutating_tool();
        assert_eq!(routing.apply_model("gpt-5.6-sol"), "gpt-5.6-sol-light");
    }

    #[test]
    fn missing_smol_does_not_switch() {
        let routing = ModelRouting::from_parts(None, true, false, None, None, None);
        routing.note_mutating_tool();
        assert!(!routing.switched());
        assert_eq!(routing.apply_model("gpt-5.6-sol"), "gpt-5.6-sol");
    }
}
