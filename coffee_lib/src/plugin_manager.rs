//! Plugin manager module definition.
use async_trait::async_trait;

use crate::{errors::CoffeeError, types::response::*};

/// Plugin manager traits that define the API a generic
/// plugin manager.
#[async_trait]
pub trait PluginManager {
    /// configure the plugin manger.
    async fn configure(&mut self) -> Result<(), CoffeeError>;

    /// install a plugin by name, return an error if some error happens.
    async fn install(
        &mut self,
        plugins: &str,
        verbose: bool,
        try_dynamic: bool,
    ) -> Result<(), CoffeeError>;

    // remove a plugin by name, return an error if some error happens.
    async fn remove(&mut self, plugin: &str) -> Result<CoffeeRemove, CoffeeError>;

    /// return the list of plugins installed by the plugin manager.
    async fn list(&mut self) -> Result<CoffeeList, CoffeeError>;

    /// upgrade a single or multiple repositories.
    async fn upgrade(&mut self, repo: &str) -> Result<CoffeeUpgrade, CoffeeError>;

    /// add the remote repository to the plugin manager.
    async fn add_remote(&mut self, name: &str, url: &str) -> Result<(), CoffeeError>;

    /// remove the remote repository from the plugin manager.
    async fn rm_remote(&mut self, name: &str) -> Result<(), CoffeeError>;

    /// list the remote repositories for the plugin manager.
    async fn list_remotes(&mut self) -> Result<CoffeeRemote, CoffeeError>;

    /// set up the core lightning configuration target for the
    /// plugin manager.
    async fn setup(&mut self, cln_conf_path: &str) -> Result<(), CoffeeError>;

    /// show the README file of the plugin
    async fn show(&mut self, plugin: &str) -> Result<CoffeeShow, CoffeeError>;

    /// clean up storage information about the remote repositories of the plugin manager.
    async fn nurse(&mut self) -> Result<(), CoffeeError>;
}
