//! The bundled samples, loaded once at start-up from `TERMINAL_SAMPLES`.
use base64::Engine;
use std::path::Path;
use terminal_core::views::{CallForm, SampleFile, SampleMeta, SampleView};
use terminal_core::{evm_call, sbpf_call};

pub fn load(dir: &Path) -> anyhow::Result<Vec<SampleView>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let d = e?.path();
        let Ok(meta_bytes) = std::fs::read(d.join("meta.json")) else { continue };
        let mut meta: SampleMeta = serde_json::from_slice(&meta_bytes)?;
        meta.default_call = match meta.id.as_str() {
            "erc20" => Some(CallForm::Evm(evm_call::erc20_transfer_form())),
            "spl-token" => Some(CallForm::Sbpf(sbpf_call::spl_transfer_form(250))),
            _ => meta.default_call,
        };
        let files = meta.files.iter().map(|n| {
            let b = std::fs::read(d.join(n))?;
            Ok(match String::from_utf8(b.clone()) {
                Ok(t) if !n.ends_with(".so") => SampleFile { name: n.clone(), encoding: "text".into(), content: t },
                _ => SampleFile { name: n.clone(), encoding: "base64".into(), content: base64::engine::general_purpose::STANDARD.encode(&b) },
            })
        }).collect::<anyhow::Result<Vec<_>>>()?;
        out.push(SampleView { meta, files });
    }
    out.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    Ok(out)
}
