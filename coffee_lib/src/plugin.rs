//! Plugin module that abstract the concept of a cln plugin
//! from a plugin manager point of view.
use std::fmt;

use log::debug;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::errors::CoffeeError;
use crate::macros::error;
use crate::plugin_conf::Conf;
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

impl PluginLang {
    pub async fn get_executable_path(
        &self,
        path: &str,
        name: &str,
        verbose: bool,
        install_requirements: bool,
    ) -> Result<String, CoffeeError> {
        match self {
            PluginLang::PyPip => {
                /* 1. RUN PIP install or poetry install
                 * 2. return the path of the main file */
                if install_requirements {
                    let script = "pip3 install -r requirements.txt";
                    sh!(path, script, verbose);
                }
                let main_file = format!("{path}/{name}.py");
                Ok(main_file)
            }
            PluginLang::PyPoetry => {
                if install_requirements {
                    let script = "pip3 install poetry \
                              poetry export -f requirements.txt --output requirements.txt \
                              pip3 install -r requirements.txt";
                    sh!(path, script, verbose);
                }
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
        }
    }
}

/// Plugin struct definition
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Plugin {
    name: String,
    /// root path of the plugin
    root_path: String,
    /// path of the main file
    pub path: String,
    lang: PluginLang,
    conf: Option<Conf>,
}

impl Plugin {
    /// create a new instance of the plugin.
    pub fn new(
        name: &str,
        root_path: &str,
        path: &str,
        plugin_lang: PluginLang,
        config: Option<Conf>,
    ) -> Self {
        Plugin {
            name: name.to_owned(),
            root_path: root_path.to_owned(),
            path: path.to_owned(),
            lang: plugin_lang,
            conf: config,
        }
    }

    /// configure the plugin in order to work with cln.
    ///
    /// In case of success return the path of the executable.
    pub async fn configure(&mut self, verbose: bool) -> Result<String, CoffeeError> {
        let exec_path = if let Some(conf) = &self.conf {
            if let Some(script) = &conf.plugin.install {
                sh!(self.root_path.clone(), script, verbose);
                format!("{}/{}", self.path, conf.plugin.main)
            } else {
                self.lang
                    .get_executable_path(&self.path, &self.name, verbose, true)
                    .await?
            }
        } else {
            self.lang
                .get_executable_path(&self.path, &self.name, verbose, true)
                .await?
        };
        Ok(exec_path)
    }

    /// upgrade the plugin to a new version.
    pub async fn upgrade(&mut self) -> Result<(), CoffeeError> {
        todo!("not implemented yet")
    }

    /// return the path of the executable
    pub async fn get_executable(&mut self) -> Result<String, CoffeeError> {
        let exec_path = if let Some(conf) = &self.conf {
            if let Some(_script) = &conf.plugin.install {
                format!("{}/{}", self.path, conf.plugin.main)
            } else {
                self.lang
                    .get_executable_path(&self.path, &self.name, false, false)
                    .await?
            }
        } else {
            self.lang
                .get_executable_path(&self.path, &self.name, false, false)
                .await?
        };
        Ok(exec_path)
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }
}

impl fmt::Display for Plugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "name: {}, path: {}", self.name, self.path)
    }
}
