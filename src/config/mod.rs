use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct AriadneConfig {
    pub project: ProjectConfig,
    pub output: OutputConfig,
    pub filters: FilterConfig,
    pub ignore: IgnoreConfig,
    pub languages: LanguagesConfig,
}

impl Default for AriadneConfig {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            output: OutputConfig::default(),
            filters: FilterConfig::default(),
            ignore: IgnoreConfig::default(),
            languages: LanguagesConfig::default(),
        }
    }
}

impl AriadneConfig {
    /// Carga `conf.ariadne`. Si `path` viene dado (ej. `--config`), es un error
    /// si no existe. Si no viene, se busca `conf.ariadne` en el cwd y, de no
    /// existir, se usan los defaults del programa sin fallar.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let resolved = match path {
            Some(p) => Some(p.to_path_buf()),
            None => {
                let default_path = PathBuf::from("conf.ariadne");
                default_path.exists().then_some(default_path)
            }
        };

        match resolved {
            Some(p) => {
                let text = std::fs::read_to_string(&p)
                    .with_context(|| format!("no se pudo leer el archivo de configuración: {}", p.display()))?;
                toml::from_str(&text)
                    .with_context(|| format!("conf.ariadne inválido en: {}", p.display()))
            }
            None => Ok(Self::default()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct ProjectConfig {
    pub name: String,
    pub path: String,
    pub description: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: ".".to_string(),
            description: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct OutputConfig {
    pub formats: Vec<String>,
    pub dir: String,
    pub html: HtmlConfig,
    pub pandoc: PandocConfig,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            formats: vec!["html".to_string()],
            dir: "./docs".to_string(),
            html: HtmlConfig::default(),
            pandoc: PandocConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct HtmlConfig {
    pub theme: String,
    pub background: String,
    pub link_color: String,
    pub text_color: String,
    pub colors: ColorsConfig,
    pub icons: IconsConfig,
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            background: "#1e1e2e".to_string(),
            link_color: "#585b70".to_string(),
            text_color: "#cdd6f4".to_string(),
            colors: ColorsConfig::default(),
            icons: IconsConfig::default(),
        }
    }
}

/// Un color por `NodeType`. `default` es el fallback para cualquier tipo sin
/// entrada explícita — incluyendo tipos futuros que aún no existen aquí.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct ColorsConfig {
    pub default: String,
    pub directory: String,
    pub file: String,
    pub class: String,
    pub object: String,
    pub attribute: String,
    pub method: String,
    pub library: String,
    pub import: String,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            default: "#89b4fa".to_string(),
            directory: "#f9e2af".to_string(),
            file: "#89b4fa".to_string(),
            class: "#a6e3a1".to_string(),
            object: "#94e2d5".to_string(),
            attribute: "#f5c2e7".to_string(),
            method: "#89dceb".to_string(),
            library: "#fab387".to_string(),
            import: "#eba0ac".to_string(),
        }
    }
}

impl ColorsConfig {
    /// Resuelve el color para un `NodeType` dado (por su nombre snake_case),
    /// cayendo a `default` si no hay entrada específica.
    pub fn resolve(&self, node_type: &str) -> &str {
        match node_type {
            "directory" => &self.directory,
            "file" => &self.file,
            "class" => &self.class,
            "object" => &self.object,
            "attribute" => &self.attribute,
            "method" => &self.method,
            "library" => &self.library,
            "import" => &self.import,
            _ => &self.default,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct IconsConfig {
    pub enabled: bool,
    pub set: String,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            set: "devicon".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct PandocConfig {
    pub binary: String,
    pub pdf_engine: String,
}

impl Default for PandocConfig {
    fn default() -> Self {
        Self {
            binary: "pandoc".to_string(),
            pdf_engine: "tectonic".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct FilterConfig {
    pub max_depth: Option<u32>,
    pub hide_generated: bool,
    pub hide_extensions: Vec<String>,
    pub hide_paths: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct IgnoreConfig {
    pub extra: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct LanguagesConfig {
    pub detect: String,
}

impl Default for LanguagesConfig {
    fn default() -> Self {
        Self {
            detect: "auto".to_string(),
        }
    }
}
