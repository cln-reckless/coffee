use std::any::Any;
use std::path::Path;

use async_trait::async_trait;
use git2;
use log::debug;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use walkdir::DirEntry;
use walkdir::WalkDir;

use coffee_lib::errors::CoffeeError;
use coffee_lib::macros::{commit_id, error, get_repo_info};
use coffee_lib::plugin::Plugin;
use coffee_lib::plugin::PluginLang;
use coffee_lib::plugin_conf::Conf;
use coffee_lib::repository::Repository;
use coffee_lib::types::response::CoffeeUpgrade;
use coffee_lib::url::URL;
use coffee_lib::utils::get_plugin_info_from_path;
use coffee_storage::model::repository::Kind;
use coffee_storage::model::repository::Repository as StorageRepository;

use crate::utils::clone_recursive_fix;
use crate::utils::git_upgrade;

pub struct Github {
    /// the url of the repository to be able
    /// to get all the plugin information.
    url: URL,
    /// the name of the repository that can be used
    /// by coffee as repository key.
    name: String,
    /// all the plugin that are listed inside the
    /// repository
    plugins: Vec<Plugin>,
    /// the name of the branch to be able to
    /// pull the changes from the correct branch
    branch: String,
    /// the latest commit id of the repository
    git_head: Option<String>,
    /// the latest commit date of the repository
    last_activity: Option<String>,
}

// FIXME: move this inside a utils dir craters
fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

struct IndexingInfo {
    config: Option<Conf>,
    lang: PluginLang,
    exec_path: String,
}

impl Github {
    /// Create a new instance of the Repository
    /// with a name and a url
    pub fn new(name: &str, url: &URL) -> Self {
        debug!("creating repository: {} {}", name, url.url_string);
        Github {
            name: name.to_owned(),
            url: url.clone(),
            plugins: vec![],
            branch: "".to_owned(),
            git_head: None,
            last_activity: None,
        }
    }

    // check if the plugin has the custom configuration to read.
    async fn lookup_config_file(
        &self,
        root_path: &str,
    ) -> Result<Option<IndexingInfo>, CoffeeError> {
        for file in ["coffee.yaml", "coffee.yml"] {
            #[allow(unused_assignments)]
            let mut plugin_lang = PluginLang::Unknown;
            let conf_path = format!("{}/{}", root_path, file);
            if let Ok(mut conf_file) = File::open(conf_path).await {
                let mut conf_str = String::new();
                conf_file.read_to_string(&mut conf_str).await?;
                log::debug!("found plugin configuration: {}", conf_str);

                let conf_file = serde_yaml::from_str::<Conf>(&conf_str)
                    .map_err(|err| error!("Coffee manifest malformed: {err}"))?;
                let conf_lang = conf_file.plugin.lang.to_owned();
                match conf_lang.as_str() {
                    "pypip" => plugin_lang = PluginLang::PyPip,
                    "pypoetry" => plugin_lang = PluginLang::PyPoetry,
                    "go" => plugin_lang = PluginLang::Go,
                    "rs" | "rust" => plugin_lang = PluginLang::Rust,
                    "dart" => plugin_lang = PluginLang::Dart,
                    "js" => plugin_lang = PluginLang::JavaScript,
                    "ts" => plugin_lang = PluginLang::TypeScript,
                    "java" | "kotlin" | "scala" => plugin_lang = PluginLang::JVM,
                    _ => {
                        return Err(error!("language {conf_lang} not supported"));
                    }
                };

                let exec_path = format!("{root_path}/{}", conf_file.plugin.main);
                return Ok(Some(IndexingInfo {
                    config: Some(conf_file),
                    lang: plugin_lang,
                    exec_path,
                }));
            }
        }
        Ok(None)
    }

    /// When a configuration file is not found this function is called
    /// and we try to guess the programming language used.
    async fn try_guess_language(
        &self,
        plugin_path: &Path,
    ) -> Result<Option<IndexingInfo>, CoffeeError> {
        log::debug!("conf file not found, so we try to guess the language");
        // try to understand the language from the file
        let files = WalkDir::new(plugin_path).max_depth(1);
        for file in files {
            let file_dir = file.unwrap().clone();
            let (derived_root_path, derived_name) = get_plugin_info_from_path(file_dir.path())?;
            let exec_path = None;
            let plugin_name = Some(derived_name.to_string());
            debug!("looking for {derived_name} in {derived_root_path}");
            let file_name = file_dir.file_name().to_str().unwrap();
            let plugin_lang = match file_name {
                "requirements.txt" => {
                    let exec_path = Some(format!("{derived_root_path}/{derived_name}.py"));
                    PluginLang::PyPip
                }
                "pyproject.toml" => {
                    let exec_path = Some(format!("{derived_root_path}/{derived_name}.py"));
                    PluginLang::PyPoetry
                }
                // We dot have any information on standard pattern on where to find the
                // plugin exec path, so for now we skip the indexing!
                //
                // N.B: The plugin should use the coffee manifest, period.
                "go.mod" => PluginLang::Go,
                "cargo.toml" => PluginLang::Rust,
                "pubspec.yaml" => PluginLang::Dart,
                "package.json" => PluginLang::JavaScript,
                "tsconfig.json" => PluginLang::TypeScript,
                _ => PluginLang::Unknown,
            };
            if plugin_lang != PluginLang::Unknown {
                // TODO: call recursive to look under sub directory
                break;
            } else {
                return Ok(Some(IndexingInfo {
                    exec_path: exec_path.unwrap(),
                    config: None,
                    lang: plugin_lang,
                }));
            }
        }
        // There is nothing in this path
        Ok(None)
    }

    /// Index the repository to store information
    /// related to the plugins
    pub async fn index_repository(&mut self) -> Result<(), CoffeeError> {
        let repo_path = &self.url.path_string;
        let target_dirs = WalkDir::new(repo_path)
            .max_depth(1)
            .into_iter()
            .filter_entry(|dir_entry| !is_hidden(dir_entry));
        let commit_id = &self.git_head;

        for plugin_dir in target_dirs {
            match plugin_dir {
                Ok(plugin_path) => {
                    let root_path = plugin_path
                        .path()
                        .as_os_str()
                        .to_os_string()
                        .to_string_lossy()
                        .to_string();
                    let mut conf = None;
                    let mut exec_path = None;
                    let mut plugin_lang = PluginLang::Unknown;
                    let mut plugin_name = None;
                    if let Some(index_info) = self.lookup_config_file(&root_path).await? {
                        exec_path = Some(index_info.exec_path);
                        plugin_lang = index_info.lang;
                        // SAFETY: it is safe to unwrap because a config gile has always a file
                        let config = index_info.config.clone().unwrap();
                        plugin_name = Some(config.plugin.name.to_owned());
                        conf = index_info.config;
                    } else {
                        let index_info = self.try_guess_language(plugin_path.path()).await?;
                    }
                    debug!("possible plugin language: {:?}", plugin_lang);
                    if exec_path.is_none() {
                        let name = plugin_name.clone().unwrap();
                        log::warn!("we are not able to find the exec path for the plugin {name} written in {:?}, so we do not index it", plugin_lang);
                        log::info!("we are not able to detect the exec path for the plugin {name}");
                        continue;
                    }

                    let Some(exec_path) = exec_path else {
                        return Err(error!(
                            "exec path not known, but we should know at this point."
                        ));
                    };

                    debug!("exec path is {exec_path}");

                    // The language is already contained inside the configuration file.
                    let plugin = Plugin::new(
                        &plugin_name.unwrap(),
                        &root_path,
                        &exec_path,
                        plugin_lang,
                        conf.clone(),
                        commit_id.clone(),
                        // The plugin for now is not installed, so it's
                        // neither enabled or disabled
                        None,
                    );

                    debug!("new plugin: {:?}", plugin);
                    self.plugins.push(plugin);
                }
                Err(err) => return Err(error!("{}", err)),
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Repository for Github {
    /// Init the repository where it is required to index
    /// all the plugin contained, and store somewhere the index.
    ///
    /// Where to store the index is an implementation
    /// details.
    async fn init(&mut self) -> Result<(), CoffeeError> {
        debug!(
            "initializing repository: {} {} > {}",
            self.name, &self.url.url_string, &self.url.path_string,
        );
        let res = git2::Repository::clone(&self.url.url_string, &self.url.path_string);
        match res {
            Ok(repo) => {
                self.branch = if repo.find_branch("master", git2::BranchType::Local).is_ok() {
                    "master".to_owned()
                } else {
                    "main".to_owned()
                };
                let (commit, date) = get_repo_info!(repo);
                self.git_head = Some(commit.clone());
                self.last_activity = Some(date.clone());

                let clone = clone_recursive_fix(repo, &self.url).await;
                self.index_repository().await?;
                clone
            }
            Err(err) => Err(error!("{}", err.message())),
        }
    }

    async fn upgrade(
        &mut self,
        plugins: &Vec<Plugin>,
        verbose: bool,
    ) -> Result<CoffeeUpgrade, CoffeeError> {
        // get the list of the plugins installed from this repository
        // TODO: add a field of installed plugins in the repository struct instead
        let mut plugins_effected: Vec<String> = vec![];
        let remote_repo = self.list().await?;

        // FIXME: upgrading the repository must also upgrade the commit
        // field of all plugins cloned from this repository.
        let plugins = plugins.clone();

        // FIXME: mark inside a repository what plugin is installed, and remove
        // this information from the configuration.
        for plugin in &remote_repo {
            if let Some(ind) = plugins
                .iter()
                .position(|elem| elem.name() == *plugin.name())
            {
                let plugin_name = &plugins[ind].name().clone();
                plugins_effected.push(plugin_name.to_owned());
            }
        }
        // pull the changes from the repository
        let status = git_upgrade(&self.url.path_string, &self.branch, verbose).await?;
        self.git_head = Some(status.commit_id());
        self.last_activity = Some(status.date());
        Ok(CoffeeUpgrade {
            repo: self.name(),
            status,
            plugins_effected,
        })
    }

    async fn recover(&mut self) -> Result<(), CoffeeError> {
        let commit = self.git_head.clone();

        log::debug!(
            "recovering repository: {} {} > {}",
            self.name,
            &self.url.url_string,
            &self.url.path_string,
        );
        // recursively clone the repository
        let res = git2::Repository::clone(&self.url.url_string, &self.url.path_string);
        match res {
            Ok(repo) => {
                // get the commit id
                let oid = git2::Oid::from_str(&commit.unwrap())
                    .map_err(|err| error!("{}", err.message()))?;
                // Retrieve the commit associated with the OID
                let target_commit = match repo.find_commit(oid) {
                    Ok(commit) => commit,
                    Err(err) => return Err(error!("{}", err.message())),
                };

                // Update HEAD to point to the target commit
                repo.set_head_detached(target_commit.id())
                    .map_err(|err| error!("{}", err.message()))?;

                // retrieve the submodules
                let submodules = repo.submodules().unwrap_or_default();
                for (_, sub) in submodules.iter().enumerate() {
                    let path =
                        format!("{}/{}", &self.url.path_string, sub.path().to_str().unwrap());
                    if let Err(err) = git2::Repository::clone(sub.url().unwrap(), &path) {
                        return Err(error!("{}", err.message()));
                    }
                }

                Ok(())
            }
            Err(err) => Err(error!("{}", err.message())),
        }
    }

    /// list of the plugin installed inside the repository.
    async fn list(&self) -> Result<Vec<Plugin>, CoffeeError> {
        Ok(self.plugins.clone())
    }

    /// name of the repository.
    fn name(&self) -> String {
        self.name.clone()
    }

    /// url of the repository.
    fn url(&self) -> URL {
        self.url.clone()
    }

    /// search inside the repository a plugin by name.
    fn get_plugin_by_name(&self, name: &str) -> Option<Plugin> {
        for plugin in &self.plugins {
            if plugin.name() == name {
                return Some(plugin.to_owned());
            }
        }
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl From<StorageRepository> for Github {
    fn from(value: StorageRepository) -> Self {
        Github {
            url: value.url,
            name: value.name,
            plugins: value.plugins,
            branch: value.branch,
            git_head: value.git_head,
            last_activity: value.last_activity,
        }
    }
}

impl From<&StorageRepository> for Github {
    fn from(value: &StorageRepository) -> Self {
        Github {
            url: value.url.to_owned(),
            name: value.name.to_owned(),
            plugins: value.plugins.to_owned(),
            branch: value.branch.to_owned(),
            git_head: value.git_head.to_owned(),
            last_activity: value.last_activity.to_owned(),
        }
    }
}

impl From<Github> for StorageRepository {
    fn from(value: Github) -> Self {
        StorageRepository {
            kind: Kind::Git,
            name: value.name,
            url: value.url,
            plugins: value.plugins,
            branch: value.branch,
            git_head: value.git_head,
            last_activity: value.last_activity,
        }
    }
}

impl From<&Github> for StorageRepository {
    fn from(value: &Github) -> Self {
        StorageRepository {
            kind: Kind::Git,
            name: value.name.to_owned(),
            url: value.url.to_owned(),
            plugins: value.plugins.to_owned(),
            branch: value.branch.to_owned(),
            git_head: value.git_head.to_owned(),
            last_activity: value.last_activity.to_owned(),
        }
    }
}
