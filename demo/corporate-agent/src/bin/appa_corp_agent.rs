use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use appa_agent::{Agent, OpenAiCompatible, Outcome, TenantId};
use appa_engine::fact::{BoundaryKind, CloseOutcome, Fact, ReturnDerivation};
use appa_engine::label::{Dim, Label};
use appa_engine::registry::TrustChain;
use appa_runtime::tool::{HttpClient, HttpTool, ToolBackend};
use appa_runtime::{Config, Limits, Mediator};
use clap::Parser;
use corp_systems::systems::System;
use corporate_agent_demo::fork_tools::{self, CorpWorld};
use corporate_agent_demo::{clean_key, load_dotenv, resolve_data_root, resolve_sink_root};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(about = "The corporate assistant on the full appa-agent loop (fork/submit_result live), tools in-process")]
struct Args {
    prompt: String,

    #[arg(long, env = "APPA_DEMO_MODEL", default_value = "openai/gpt-5.6-luna")]
    model: String,

    #[arg(long, env = "OPENROUTER_API_KEY")]
    api_key: Option<String>,

    #[arg(long)]
    data_root: Option<PathBuf>,

    #[arg(long)]
    sink_root: Option<PathBuf>,

    #[arg(long)]
    policy: PathBuf,

    #[arg(long, default_value_t = 8)]
    max_forks: u32,

    #[arg(long, default_value_t = 1)]
    max_fork_depth: u32,

    #[arg(long)]
    quiet: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dotenv = load_dotenv();
    let args = Args::parse();
    if !args.quiet
        && let Some(path) = &dotenv
    {
        eprintln!("loaded env from {}", path.display());
    }

    let api_key = args
        .api_key
        .as_deref()
        .map(clean_key)
        .filter(|key| !key.is_empty())
        .context(
            "no OpenRouter API key: pass --api-key, set OPENROUTER_API_KEY, or add it to a .env file \
             (see .env.example)",
        )?;

    let policy_path = args.policy;
    let policy_text = std::fs::read_to_string(&policy_path)
        .with_context(|| format!("reading the policy file {}", policy_path.display()))?;
    let config =
        Config::from_toml_str(&policy_text).with_context(|| format!("loading the policy {}", policy_path.display()))?;
    let chain = config.registry_config().trust_chain.clone();

    let enabled = match std::env::var("CORP_ENABLED_SYSTEMS") {
        Ok(list) if !list.trim().is_empty() => System::parse_list(&list).context("parsing CORP_ENABLED_SYSTEMS")?,
        _ => System::ALL.into_iter().collect(),
    };
    let world = CorpWorld {
        data_root: resolve_data_root(args.data_root),
        sink_root: resolve_sink_root(args.sink_root),
        enabled,
    };
    let address = fork_tools::serve(world)
        .await
        .context("binding the loopback tool shim")?;
    let shim_url = format!("http://{address}/");

    let client = HttpClient::loopback();
    let backends: BTreeMap<_, _> = config
        .registry_config()
        .tools
        .iter()
        .map(|contract| {
            let backend = ToolBackend::Http(HttpTool::new(shim_url.clone(), Duration::from_secs(15), client.clone()));
            (contract.name.clone(), backend)
        })
        .collect();
    if !args.quiet {
        eprintln!(
            "appa: policy {} — {} tools in-process at {shim_url}",
            policy_path.display(),
            backends.len()
        );
    }

    let mediator = Arc::new(Mediator::with_tool_backends(config, backends).context("assembling the mediator")?);
    let limits = Limits {
        max_inference_rounds: 24,
        run_deadline: Duration::from_secs(240),
        max_forks: args.max_forks,
        max_fork_depth: args.max_fork_depth,
        ..Limits::default()
    };
    let agent = Agent::new(
        mediator.clone(),
        OpenAiCompatible::openrouter(args.model.clone(), api_key),
        limits,
    );

    let tenant = TenantId::new("appa-corp-agent");
    let (session, outcome) = agent
        .run_new(tenant.clone(), args.prompt, CancellationToken::new())
        .await
        .context("driving the root turn")?;

    let (facts, _revision) = mediator.snapshot(&tenant, &session).context("reading the family log")?;
    if !args.quiet {
        replay(&facts, &chain);
    }

    match outcome {
        Outcome::Final(text) => {
            println!("\n=== answer ===\n{text}");
            Ok(())
        }
        Outcome::ChildFinished => anyhow::bail!("the run ended inside a child session"),
        Outcome::PolicyStop(message) => {
            println!("\n=== answer ===\n(no answer: {message})");
            anyhow::bail!("policy stop: {message}")
        }
    }
}

fn replay(facts: &[Fact], chain: &TrustChain) {
    for fact in facts {
        match fact {
            Fact::AssistantMessage {
                trajectory,
                content,
                calls,
            } => {
                for call in calls {
                    eprintln!(
                        "appa: [{}] proposes {}({})",
                        trajectory.as_str(),
                        call.tool.as_str(),
                        call.arguments
                    );
                }
                if calls.is_empty()
                    && let Some(text) = content
                {
                    eprintln!("appa: [{}] answers: {text}", trajectory.as_str());
                }
            }
            Fact::BlockFeedback {
                trajectory, content, ..
            } => {
                eprintln!("appa: [{}] block feedback: {content}", trajectory.as_str());
            }
            Fact::Ruling {
                trajectory, authority, ..
            } => {
                eprintln!(
                    "appa: remedy authorized [{}]: ruling by {}",
                    trajectory.as_str(),
                    authority.as_str()
                );
            }
            Fact::Acceptance {
                trajectory, narrowing, ..
            } => {
                eprintln!(
                    "appa: remedy authorized [{}]: narrowing accepted {} -> {}",
                    trajectory.as_str(),
                    label_text(&narrowing.from, chain),
                    label_text(&narrowing.to, chain)
                );
            }
            Fact::ChildReturnAcceptance {
                trajectory, narrowing, ..
            } => {
                eprintln!(
                    "appa: remedy authorized [{}]: child-return narrowing accepted {} -> {}",
                    trajectory.as_str(),
                    label_text(&narrowing.from, chain),
                    label_text(&narrowing.to, chain)
                );
            }
            Fact::ChildReturn {
                trajectory, derivation, ..
            } => match derivation {
                ReturnDerivation::Raw => {
                    eprintln!("appa: [{}] child return crossed raw", trajectory.as_str());
                }
                ReturnDerivation::Sanitized { sanitizer, .. } => {
                    eprintln!(
                        "appa: remedy authorized [{}]: child return crossed as the {} derivation",
                        trajectory.as_str(),
                        sanitizer.as_str()
                    );
                }
            },
            Fact::ValueAdmitted { trajectory, value, .. } => {
                eprintln!(
                    "appa: [{}] value admitted at {}",
                    trajectory.as_str(),
                    label_text(&value.label, chain)
                );
            }
            Fact::DispatchClosed {
                trajectory, outcome, ..
            } => {
                let closed = match outcome {
                    CloseOutcome::Success { effects } if effects.is_empty() => "ran".to_string(),
                    CloseOutcome::Success { effects } => {
                        let kinds: Vec<&str> = effects.iter().map(|kind| kind.as_str()).collect();
                        format!("ran, committing [{}]", kinds.join(", "))
                    }
                    CloseOutcome::Failure => "failed".to_string(),
                    CloseOutcome::Indeterminate => "may or may not have run".to_string(),
                };
                eprintln!("appa: [{}] dispatch {closed}", trajectory.as_str());
            }
            Fact::Boundary { trajectory, kind } => match kind {
                BoundaryKind::Fork { parent, seed, .. } => eprintln!(
                    "appa: [{}] forked from {} at {}",
                    trajectory.as_str(),
                    parent.as_str(),
                    label_text(seed, chain)
                ),
                BoundaryKind::Merge { .. } => {
                    eprintln!("appa: [{}] merged a child return", trajectory.as_str());
                }
                BoundaryKind::TurnEnd => {}
            },
            _ => {}
        }
    }
}

fn label_text(label: &Label, chain: &TrustChain) -> String {
    let trust = match &label.trust {
        Dim::Known(trust) => chain.name_of(*trust).unwrap_or("?").to_string(),
        Dim::Unknown => "unknown".to_string(),
    };
    let audience = match &label.audience {
        Dim::Known(audience) => format!("{audience:?}"),
        Dim::Unknown => "unknown".to_string(),
    };
    format!("trust={trust} audience={audience}")
}
