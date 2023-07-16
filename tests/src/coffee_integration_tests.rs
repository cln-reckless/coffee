use std::sync::Arc;

use serde_json::json;

use coffee_lib::plugin_manager::PluginManager;
use coffee_testing::cln::Node;
use coffee_testing::prelude::tempfile;
use coffee_testing::{CoffeeTesting, CoffeeTestingArgs};

use crate::init;

#[tokio::test]
pub async fn init_coffee_test() -> anyhow::Result<()> {
    init();

    let mut manager = CoffeeTesting::tmp().await?;
    let result = manager.coffee().list().await?;
    assert!(
        result.plugins.is_empty(),
        "list of plugin not empty: {:?}",
        result
    );
    Ok(())
}

#[tokio::test]
pub async fn init_coffee_test_cmd() -> anyhow::Result<()> {
    init();

    let dir = Arc::new(tempfile::tempdir()?);
    let args = CoffeeTestingArgs {
        conf: None,
        data_dir: dir.path().clone().to_str().unwrap().to_owned(),
        network: "bitcoin".to_string(),
    };
    let mut manager = CoffeeTesting::tmp_with_args(&args, dir.clone()).await?;
    let root_path = manager.root_path().to_owned();
    manager
        .coffee()
        .add_remote("folgore", "https://github.com/coffee-tools/folgore.git")
        .await
        .unwrap();

    // dropping the first coffee instance, but without delete the dir
    drop(manager);
    let new_args = CoffeeTestingArgs {
        conf: None,
        data_dir: dir.path().clone().to_string_lossy().to_string(),
        network: "testnet".to_string(),
    };
    let mut manager = CoffeeTesting::tmp_with_args(&new_args, dir.clone()).await?;
    let new_root_path = manager.root_path().to_owned();
    assert_eq!(root_path.path(), new_root_path.path());

    let actual_network = manager.coffee().storage_info().config.network;
    let expected_network = "testnet".to_string();

    assert_eq!(
        actual_network, expected_network,
        "Network is wrong. Actual: '{actual_network}', Expected: '{expected_network}'"
    );
    Ok(())
}

#[tokio::test]
pub async fn init_coffee_test_with_cln() -> anyhow::Result<()> {
    init();
    let cln = Node::tmp("regtest").await?;

    let mut manager = CoffeeTesting::tmp().await?;
    let result = manager.coffee().list().await?;
    assert!(
        result.plugins.is_empty(),
        "list of plugin not empty: {:?}",
        result
    );
    let lightning_dir = cln.rpc().getinfo()?.ligthning_dir;
    let lightning_dir = lightning_dir.strip_suffix("/regtest").unwrap();
    log::info!("lightning path: {lightning_dir}");

    manager.coffee().setup(&lightning_dir).await?;

    Ok(())
}

#[tokio::test]
//#[ntest::timeout(120000)]
pub async fn init_coffee_test_add_remote() {
    init();
    let mut cln = Node::tmp("regtest").await.unwrap();

    let mut manager = CoffeeTesting::tmp().await.unwrap();
    let result = manager.coffee().list().await.unwrap();
    assert!(
        result.plugins.is_empty(),
        "list of plugin not empty: {:?}",
        result
    );
    let lightning_dir = cln.rpc().getinfo().unwrap().ligthning_dir;
    let lightning_dir = lightning_dir.strip_suffix("/regtest").unwrap();
    log::info!("lightning path: {lightning_dir}");

    manager.coffee().setup(&lightning_dir).await.unwrap();

    manager
        .coffee()
        .add_remote("lightningd", "https://github.com/lightningd/plugins.git")
        .await
        .unwrap();
    manager
        .coffee()
        .install("summary", true, true)
        .await
        .unwrap();

    cln.rpc()
        .call::<serde_json::Value, serde_json::Value>("summary", json!({}))
        .unwrap();

    cln.stop().await.unwrap();
}

#[tokio::test]
#[ntest::timeout(120000)]
pub async fn test_add_remove_plugins() {
    init();

    let mut cln = Node::tmp("regtest").await.unwrap();
    let mut manager = CoffeeTesting::tmp().await.unwrap();

    let lightning_dir = cln.rpc().getinfo().unwrap().ligthning_dir;
    let lightning_dir = lightning_dir.strip_suffix("/regtest").unwrap();
    log::info!("lightning path: {lightning_dir}");

    manager.coffee().setup(&lightning_dir).await.unwrap();
    // Add lightningd remote repository
    let repo_name = "lightningd";
    let repo_url = "https://github.com/lightningd/plugins.git";
    manager
        .coffee()
        .add_remote(repo_name, repo_url)
        .await
        .unwrap();

    // Install summary plugin
    let result = manager.coffee().install("summary", true, false).await;
    assert!(result.is_ok(), "{:?}", result);

    // Install helpme plugin
    manager
        .coffee()
        .install("helpme", true, false)
        .await
        .unwrap();

    // Ensure that the list of remotes is correct
    let result = manager.coffee().list_remotes().await;
    assert!(result.is_ok(), "list_remotes failed. result: {:?}", result);
    let remotes = result.unwrap().remotes.unwrap();
    log::debug!("remotes: {:?}", remotes);
    assert_eq!(remotes.len(), 1, "Unexpected number of remote repositories");
    assert!(
        remotes.iter().any(|remote| remote.local_name == repo_name),
        "Remote repository 'lightningd' not found"
    );

    // Ensure that the list of plugins is correct
    let result = manager.coffee().list().await;
    assert!(result.is_ok(), "{:?}", result);
    let plugins = result.unwrap().plugins;
    log::debug!("plugins: {:?}", plugins);
    assert_eq!(plugins.len(), 2, "{:?}", plugins);
    assert!(
        plugins.iter().any(|plugin| plugin.name() == "summary"),
        "Plugin 'summary' not found"
    );
    assert!(
        plugins.iter().any(|plugin| plugin.name() == "helpme"),
        "Plugin 'helpme' not found"
    );

    // Remove summary plugin
    let result = manager.coffee().remove("summary").await;
    assert!(result.is_ok(), "{:?}", result);

    // Ensure that the list of plugins is correct
    let result = manager.coffee().list().await;
    assert!(result.is_ok(), "{:?}", result);

    let plugins = result.unwrap().plugins;
    assert_eq!(plugins.len(), 1, "{:?}", plugins);
    assert!(
        plugins.iter().any(|plugin| plugin.name() == "helpme"),
        "Plugin 'helpme' not found"
    );

    // Remove lightningd remote repository
    // This should also remove the helpme plugin
    let result = manager.coffee().rm_remote(repo_name).await;
    assert!(result.is_ok(), "{:?}", result);

    // Ensure that the list of remotes is correct
    let result = manager.coffee().list_remotes().await;
    assert!(result.is_ok(), "{:?}", result);

    let remotes = result.unwrap().remotes.clone();
    assert!(remotes.is_some(), "{:?}", remotes);
    let remotes = remotes.unwrap();
    assert_eq!(remotes.len(), 0, "{:?}", remotes);

    // Ensure that the list of plugins is correct
    let result = manager.coffee().list().await;
    assert!(result.is_ok(), "{:?}", result);
    let plugins = result.unwrap().plugins;
    log::debug!("plugins: {:?}", plugins);
    assert_eq!(plugins.len(), 0, "{:?}", plugins);

    cln.stop().await.unwrap();
}

#[tokio::test]
#[ntest::timeout(120000)]
pub async fn test_errors_and_show() {
    init();

    let mut cln = Node::tmp("regtest").await.unwrap();
    let mut manager = CoffeeTesting::tmp().await.unwrap();

    let lightning_dir = cln.rpc().getinfo().unwrap().ligthning_dir;
    let lightning_dir = lightning_dir.strip_suffix("/regtest").unwrap();
    log::info!("lightning path: {lightning_dir}");

    manager.coffee().setup(&lightning_dir).await.unwrap();
    // Add lightningd remote repository
    let repo_name = "lightningd";
    let repo_url = "https://github.com/lightningd/plugins.git";
    manager
        .coffee()
        .add_remote(repo_name, repo_url)
        .await
        .unwrap();

    // Install summary plugin
    let result = manager.coffee().install("summary", true, false).await;
    assert!(result.is_ok(), "{:?}", result);

    // Get the README file for a plugin that is not installed
    let result = manager.coffee().show("helpme").await.unwrap();
    let val = result.readme.as_str();
    assert!(val.starts_with("# Helpme plugin"));

    // Install a plugin that is not in the repository
    let result = manager.coffee().install("x", true, false).await;
    assert!(result.is_err(), "{:?}", result);

    // Remove helpme plugin
    // This should fail because it is not installed
    let result = manager.coffee().remove("helpme").await;
    assert!(result.is_err(), "{:?}", result);

    // Remove folgore remote repository
    // This should also fail
    let result = manager.coffee().rm_remote("folgore").await;
    assert!(result.is_err(), "{:?}", result);

    // Ensure that the list of remotes is correct
    let result = manager.coffee().list_remotes().await;
    assert!(result.is_ok(), "list_remotes failed. result: {:?}", result);
    let remotes = result.unwrap().remotes.unwrap();
    log::debug!("remotes: {:?}", remotes);
    assert_eq!(remotes.len(), 1, "{:?}", remotes);
    assert!(
        remotes.iter().any(|remote| remote.local_name == repo_name),
        "{:?}",
        remotes
    );

    // Ensure that the list of plugins is correct
    let result = manager.coffee().list().await;
    assert!(result.is_ok(), "{:?}", result);
    let plugins = result.unwrap().plugins;
    log::debug!("plugins: {:?}", plugins);
    assert_eq!(plugins.len(), 1, "{:?}", plugins);
    assert!(
        plugins.iter().any(|plugin| plugin.name() == "summary"),
        "{:?}",
        plugins
    );

    cln.stop().await.unwrap();
}

#[tokio::test]
pub async fn install_plugin_in_two_networks() -> anyhow::Result<()> {
    init();
    // initialize a lightning node in regtest network
    let mut cln = Node::tmp("regtest").await.unwrap();
    let lightning_dir = cln.rpc().getinfo().unwrap().ligthning_dir;
    let lightning_regtest_dir = lightning_dir.strip_suffix("/regtest").unwrap();
    log::info!("lightning path for regtest network: {lightning_dir}");

    let dir = Arc::new(tempfile::tempdir()?);
    let args = CoffeeTestingArgs {
        conf: None,
        data_dir: dir.path().clone().to_str().unwrap().to_owned(),
        network: "regtest".to_string(),
    };
    let mut manager = CoffeeTesting::tmp_with_args(&args, dir.clone()).await?;
    let root_path = manager.root_path().to_owned();
    let result = manager.coffee().setup(&lightning_regtest_dir).await;
    assert!(result.is_ok(), "{:?}", result);
    // Add lightningd remote repository
    manager
        .coffee()
        .add_remote("lightningd", "https://github.com/lightningd/plugins.git")
        .await
        .unwrap();
    // Install summary plugin
    // This should install summary plugin for regtest network
    manager
        .coffee()
        .install("summary", true, true)
        .await
        .unwrap();
    // Ensure that summary is installed for regtest network
    cln.rpc()
        .call::<serde_json::Value, serde_json::Value>("summary", json!({}))
        .unwrap();
    cln.stop().await.unwrap();

    // dropping the first coffee instance, but without delete the dir
    drop(manager);
    log::info!("------ second run -------");
    // initialize a lightning node in testnet network
    let mut cln = Node::tmp("testnet").await.unwrap();
    let lightning_dir = cln.rpc().getinfo().unwrap().ligthning_dir;
    let lightning_testnet_dir = lightning_dir.strip_suffix("/testnet").unwrap();
    log::info!("lightning path for testnet network: {lightning_dir}");
    let new_args = CoffeeTestingArgs {
        conf: None,
        data_dir: dir.path().clone().to_string_lossy().to_string(),
        network: "testnet".to_string(),
    };
    let mut manager = CoffeeTesting::tmp_with_args(&new_args, dir.clone()).await?;
    let new_root_path = manager.root_path().to_owned();
    assert_eq!(dir.path(), new_root_path.path());
    manager
        .coffee()
        .setup(&lightning_testnet_dir)
        .await
        .unwrap();

    let result = manager
        .coffee()
        .add_remote("lightningd", "https://github.com/lightningd/plugins.git")
        .await;
    assert!(result.is_err(), "{:?}", result);
    // Install summary plugin
    // This should install summary plugin for testnet network
    manager
        .coffee()
        .install("summary", true, true)
        .await
        .unwrap();
    // Ensure that summary is installed for testnet network
    cln.rpc()
        .call::<serde_json::Value, serde_json::Value>("summary", json!({}))
        .unwrap();

    cln.stop().await.unwrap();
    Ok(())
}
