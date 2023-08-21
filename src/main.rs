pub mod args;
pub mod errors;

use crate::errors::*;
use clap::Parser;
use env_logger::Env;
use llm::Model;
use std::borrow::Cow;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};

fn find_from_directory(path: &Path) -> Result<Option<PathBuf>> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else { continue };

        if !file_name.ends_with(".bin") {
            continue;
        }
        if !file_name.contains("-chat") {
            continue;
        }

        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            continue;
        }
        return Ok(Some(entry.path()));
    }
    Ok(None)
}

fn find_llama_model() -> Result<Cow<'static, Path>> {
    for path in [
        Path::new("/usr/lib/llama/llama-2-7b-chat.ggmlv3.q4_1.bin"),
        Path::new("/usr/lib/llama/llama-2-13b-chat.ggmlv3.q4_1.bin"),
        Path::new("/usr/lib/llama"),
    ] {
        match fs::metadata(path) {
            Ok(m) if m.is_dir() => match find_from_directory(path) {
                Ok(Some(path)) => return Ok(Cow::Owned(path)),
                Ok(None) => (),
                Err(err) => warn!("Failed to access directory: {path:?}: {err:#}"),
            },
            Ok(_) => return Ok(Cow::Borrowed(path)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => warn!("Failed to access: {path:?}: {err:#}"),
        }
    }

    bail!("Failed to find any available llama models")
}

fn main() -> anyhow::Result<()> {
    let args = args::Args::parse();

    let log_level = match args.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    env_logger::init_from_env(Env::default().default_filter_or(log_level));

    let model_path = if let Some(path) = args.model_path {
        Cow::Owned(path)
    } else {
        find_llama_model()?
    };
    info!("Using llama model: {model_path:?}");

    // ensure we can open the text
    let mut text_reader: Box<dyn Read> = if args.path == Path::new("-") {
        Box::new(io::stdin())
    } else {
        let file = fs::File::open(&args.path)
            .with_context(|| anyhow!("Failed to open file: {:?}", args.path))?;
        Box::new(file)
    };

    // load a GGML model from disk
    let llama = llm::load::<llm::models::Llama>(
        &model_path,
        llm::TokenizerSource::Embedded,
        llm::ModelParameters {
            // default: 2048 (https://docs.rs/llm/latest/llm/struct.ModelParameters.html#impl-Default-for-ModelParameters)
            context_size: 2_usize.pow(args.context_size),
            use_gpu: true,
            ..Default::default()
        },
        |progress| {
            debug!("Loading llm: {progress:?}");
        },
    )
    .with_context(|| anyhow!("Failed to load model: {model_path:?}"))?;

    // build prompt
    let mut text = String::new();
    text_reader
        .read_to_string(&mut text)
        .context("Failed to read utf-8 text")?;

    let prompt = format!(
        "[INST] <<SYS>>
Summarize this
<</SYS>>

{text}
[/INST]
"
    );

    // use the model to generate text from a prompt
    let mut session = llama.start_session(Default::default());
    let stats = session.infer::<std::convert::Infallible>(
        &llama,
        // randomness provider
        &mut rand::thread_rng(),
        &llm::InferenceRequest {
            prompt: llm::Prompt::Text(&prompt),
            parameters: &llm::InferenceParameters::default(),
            play_back_previous_tokens: false,
            maximum_token_count: None,
        },
        // llm::OutputRequest
        &mut Default::default(),
        // output callback
        |r| match r {
            llm::InferenceResponse::PromptToken(t) | llm::InferenceResponse::InferredToken(t) => {
                let mut stdout = io::stdout();
                let Ok(_) = stdout.write_all(t.as_bytes()) else { return Ok(llm::InferenceFeedback::Halt) };
                let Ok(_) = stdout.flush() else { return Ok(llm::InferenceFeedback::Halt) };
                Ok(llm::InferenceFeedback::Continue)
            }
            _ => Ok(llm::InferenceFeedback::Continue),
        }
    ).context("Failed to infer from text")?;

    let mut stdout = io::stdout();
    stdout.write_all(b"\n").ok();

    for stat in stats.to_string().lines() {
        info!("Inference stat: {stat}");
    }

    Ok(())
}
