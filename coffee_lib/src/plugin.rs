//! Plugin module that abstract the concept of a cln plugin
//! from a plugin manager point of view.
use std::fmt::{self, Display};
use std::fs::{File, Permissions};
use std::io::Write;
use std::os::unix::prelude::PermissionsExt;

use log;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::errors::CoffeeError;
use crate::macros::error;
use crate::plugin_conf::{Conf, Tipping};
use crate::sh;

/// Plugin language definition
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PluginLang {
    PyPip,
    PyPoetry,
    Go,
    Rust,
    Dart,
    JVM,
    JavaScript,
    TypeScript,
    Unknown,
}

impl Display for PluginLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lang = match self {
            PluginLang::PyPip | PluginLang::PyPoetry => "python",
            PluginLang::JavaScript => "javascript",
            PluginLang::Rust => "rust",
            PluginLang::Dart => "dart",
            PluginLang::Go => "go",
            PluginLang::JVM => "jvm",
            PluginLang::TypeScript => "typescrip",
            PluginLang::Unknown => "unknown",
        };
        write!(f, "{lang}")
    }
}

impl PluginLang {
    fn docker_file(&self) -> Option<&str> {
        match self {
            PluginLang::PyPip => Some(include_str!("../docker/pip.Dockerfile")),
            PluginLang::PyPoetry => Some(include_str!("../docker/poetry.Dockerfile")),
            _ => None,
        }
    }

    pub async fn default_install(
        &self,
        path: &str,
        name: &str,
        verbose: bool,
    ) -> Result<String, CoffeeError> {
        match self.docker_file() {
            Some(docker_file) => {
                let container_name = format!("plugin.{name}");
                log::debug!("Using dockerfile: \n{docker_file}");
                let script =
                    format!("echo \"{docker_file}\" | docker build . -f - -t {container_name}");

                sh!(path, script, verbose);
                let exec = format!("{path}/{name}.sh");

                log::debug!("{}", exec);

                let args = match self {
                    PluginLang::PyPip | PluginLang::PyPoetry => format!("python {name}.py"),
                    _ => String::new(),
                };

                let mut f = File::create(&exec)?;
                f.write_all(format!("docker run {container_name} {args}").as_bytes())
                    .map_err(|_| error!("could not write executable file"))?;

                f.set_permissions(Permissions::from_mode(0o755))
                    .map_err(|_| error!("could not set permissions for executable file"))?;

                Ok(exec)
            }
            None => match self {
                PluginLang::PyPip => {
                    /* 1. RUN PIP install or poetry install
                     * 2. return the path of the main file */
                    let script = "pip3 install -r requirements.txt --break-system-packages";
                    sh!(path, script, verbose);
                    let main_file = format!("{path}/{name}.py");
                    Ok(main_file)
                }
                PluginLang::PyPoetry => {
                    let mut script = "pip3 install poetry\n".to_string();
                    script += "poetry export -f requirements.txt --output requirements.txt\n";
                    script += "pip3 install -r requirements.txt";
                    sh!(path, script, verbose);
                    Ok(format!("{path}/{name}.py"))
                }
                PluginLang::Go => Err(error!(
                    "golang is not supported as default language, please us the coffee.yml manifest"
                )),
                PluginLang::Rust => Err(error!(
                    "rust is not supported as default language, please use the coffee.yml manifest"
                )),
                PluginLang::Dart => Err(error!(
                    "dart is not supported as default language, please use the cofee.yml manifest"
                )),
                PluginLang::JavaScript => Err(error!(
                    "js is not supported as default language, please use the coffee.yml manifest"
                )),
                PluginLang::TypeScript => Err(error!(
                    "ts is not supported as default language, please use the coffee.yml manifest"
                )),
                PluginLang::JVM => Err(error!(
                    "JVM is not supported as default language, please use the coffee.yml manifest"
                )),
                PluginLang::Unknown => {
                    /* 1. emit an error message  */
                    Err(error!(
                        "unknown default install procedure, the language in undefined"
                    ))
                }
            },
        }
    }
}

/// Plugin struct definition
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Plugin {
    name: String,
    /// root path of the plugin
    pub root_path: String,
    /// path of the main file
    pub exec_path: String,
    pub lang: PluginLang,
    conf: Option<Conf>,
    /// FIXME: this field shouldn't be optional
    pub commit: Option<String>,
    // Optional for now to be backward compatible
    /// If the plugin is enabled or not
    pub enabled: Option<bool>,
}

impl Plugin {
    /// create a new instance of the plugin.
    pub fn new(
        name: &str,
        root_path: &str,
        path: &str,
        plugin_lang: PluginLang,
        config: Option<Conf>,
        commit_id: Option<String>,
        enabled: Option<bool>,
    ) -> Self {
        Plugin {
            name: name.to_owned(),
            root_path: root_path.to_owned(),
            exec_path: path.to_owned(),
            lang: plugin_lang,
            conf: config,
            commit: commit_id,
            enabled,
        }
    }

    /// configure the plugin in order to work with cln.
    ///
    /// In case of success return the path of the executable.
    pub async fn configure(&mut self, verbose: bool) -> Result<String, CoffeeError> {
        log::debug!("install plugin inside from root dir {}", self.root_path);

        let conf = self.conf.as_ref();
        match conf.and_then(|conf| conf.plugin.install.clone()) {
            Some(script) => sh!(self.root_path.clone(), script, verbose),
            None => {
                self.exec_path = self
                    .lang
                    .default_install(&self.root_path, &self.name, verbose)
                    .await?;
            }
        };

        Ok(self.exec_path.clone())
    }

    /// remove the plugin and clean up all the data.
    async fn remove(&mut self) -> Result<(), CoffeeError> {
        todo!("not implemented yet")
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn tipping_info(&self) -> Option<Tipping> {
        self.conf.as_ref().and_then(|conf| conf.tipping.clone())
    }

    pub fn important(&self) -> bool {
        if let Some(config) = &self.conf {
            if let Some(important) = config.plugin.important {
                important
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl fmt::Display for Plugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "name: {}, path: {}", self.name, self.exec_path)
    }
}
