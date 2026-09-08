use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub struct PandocOptions<'a> {
    pub binary: &'a str,
    pub pdf_engine: &'a str,
}

/// Verifica que el binario de Pandoc esté disponible, con un mensaje
/// accionable si no lo está (falla rápido, antes de intentar conversiones).
pub fn check_available(binary: &str) -> Result<()> {
    let status = Command::new(binary)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => bail!(
            "no se encontró '{binary}' en PATH. Instala Pandoc (https://pandoc.org/installing.html) \
             o ajusta [output.pandoc].binary en conf.ariadne"
        ),
    }
}

/// Convierte `markdown` al `format` pedido invocando Pandoc como subproceso,
/// pasando el markdown por stdin y escribiendo el resultado en `out_path`.
pub fn convert(markdown: &str, format: &str, out_path: &Path, opts: &PandocOptions) -> Result<()> {
    let mut cmd = Command::new(opts.binary);
    cmd.arg("-f").arg("markdown").arg("-o").arg(out_path);

    match format {
        "pdf" => {
            cmd.arg("--pdf-engine").arg(opts.pdf_engine);
        }
        "xml" => {
            cmd.arg("-t").arg("docbook");
        }
        "rtf" => {
            cmd.arg("-t").arg("rtf");
        }
        "latex" => {
            cmd.arg("-t").arg("latex");
        }
        other => bail!("formato de documento no soportado por Pandoc: {other}"),
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "no se pudo ejecutar '{}': ¿está pandoc instalado y en PATH?",
                opts.binary
            )
        })?;

    {
        let stdin = child.stdin.as_mut().expect("stdin fue configurado como piped");
        stdin
            .write_all(markdown.as_bytes())
            .context("no se pudo escribir el markdown a pandoc")?;
    }

    let output = child.wait_with_output().context("pandoc falló al ejecutarse")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pandoc terminó con error generando '{format}': {stderr}");
    }

    Ok(())
}
