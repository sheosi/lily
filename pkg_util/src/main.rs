use std::collections::HashMap;
use std::env::current_dir;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{anyhow, Result};
use bytes::Bytes;
use clap::{Arg, App, AppSettings, SubCommand};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use zip::ZipArchive;

const DEF_REPO_URL: &str = "";
const PKG_PATH: &str = "../packages";


#[tokio::main(flavor="current_thread")]
async fn main() -> anyhow::Result<()> {
    let matches = App::new("Package utility")
                  .version("0.3")
                  .setting(AppSettings::SubcommandRequiredElseHelp)
                  .subcommand(SubCommand::with_name("init")
                              .arg(
                                  Arg::with_name("pkg_name").required(true)
                                )
                            )
                   .subcommand(SubCommand::with_name("install")
                              .arg(
                                  Arg::with_name("pkg_name").required(true)
                                )
                            )
                    .subcommand(SubCommand::with_name("remove")
                               .arg(
                                    Arg::with_name("pkg_name").required(true)
                               )
                            )
                  .get_matches();

    if let Some(init_data) = matches.subcommand_matches("init") {
        do_init(init_data.value_of("pkg_name").expect("Couldn't get pkg_name but is required... Wut?"))?;
    }
    else if let Some(install_data) = matches.subcommand_matches("install") {
        let repo = RepoData::get_from(DEF_REPO_URL).await?;
        if let Err(e) = repo.install(Path::new(PKG_PATH),install_data.value_of("pkg_name").expect("Couldn't get pkg_name but is required... Wut?")).await {
            match e.downcast::<RepoError>() {
                Ok(e) => {eprintln!("{}", e)},
                Err(e) => {Err(e)?}
            }
        }
    }
    else if let Some(remove_data) = matches.subcommand_matches("remove") {
        let path = Path::new(PKG_PATH);
        let pkg_name = remove_data.value_of("pkg_name").expect("Couldn't get pkg_name but is required... Wut?");
        let pkg_path = path.join(pkg_name);
        if let Err(e) = fs::remove_dir_all(pkg_path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Package \"{}\" does not exist", pkg_name);
            }
            else {Err(e)?}
        }
    }

    Ok(())
}

fn first_upper(mut input: String) -> String {
    let mut new = input.split_off(1).to_uppercase();
    new.push_str(&input);
    new
}

fn do_init(pkg_path_str: &str) -> Result<()> {
    // Create main folder
    let pkg_path = Path::new(pkg_path_str);
    let pkg_name = pkg_path
                    .file_name().ok_or_else(||anyhow!("Path must follow in some name"))?
                    .to_str().ok_or_else(||anyhow!("Package name must be valid Unicode"))?;
    

    // If the folder exists, stop right on the track
    if pkg_path.exists() {
        return Err(anyhow!("Package already exists"))
    }

    fs::create_dir_all(&pkg_path)?;

    // Create Python module
    let py_mod_path = pkg_path.join("python").join(pkg_name);
    fs::create_dir_all(&py_mod_path)?;
    let mut py_file = fs::File::create(py_mod_path.join("__init__.py"))?;
    write!(&mut py_file, "from lily_ext import action, answer, conf, translate

@action(name = \"{}\")
class {}:

    def __init__(self):
        pass

    def trigger_action(self, args, context):
        pass

    ", pkg_name.to_lowercase(), first_upper(pkg_name.to_owned()))?;

    // Create translation
    let trans_path = pkg_path.join("translations");
    fs::create_dir(&trans_path)?;

    let translations = &[
        ("en-US", "example_translation_say = Hello there, {$friend_name}
        ")
    ];
    for (trans_lang, trans_demo) in translations {
        let lang_path = trans_path.join(trans_lang);
        fs::create_dir(&lang_path)?;

        let mut trans_file = fs::File::create(lang_path.join("main.ftl"))?;
        write!(&mut trans_file, "{}", trans_demo)?;
    }

    // Skills definition file
    let mut skills_def = fs::File::create(pkg_path.join("skills_def.yaml"))?;
    write!(&mut skills_def, "example:
    signals:
        order: 
        text: \"Say hello to {{$friend_name}}\"
        entities:
            friend_name:
            kind:
                data:
                - Alex
                - John
            example: Alex
    actions:
        say: $example_translation_say
")?;

    print_path_cute(pkg_name,&pkg_path)?;

    Ok(())
}

fn print_path_cute(pkg_name: &str, pkg_path: &Path) -> Result<()> {
    let current_dir = current_dir()?;
    if let Some(parent_path) = pkg_path.parent() {

        // Make sure parent path is absolute so that canonicalize 
        // doesn't complain
        if parent_path != current_dir  {
            let parent_path = if parent_path.is_absolute() {
                parent_path.to_path_buf()
            }
            else {
                let mut absolute_path = current_dir.clone();
                absolute_path.push(parent_path);
                absolute_path
            };
            println!("\tCreated package \"{}\" at \"{}\"", pkg_name, parent_path.canonicalize()?.to_string_lossy());
        }
        else {
            println!("\tCreated package \"{}\"", pkg_name);
        }    
    }
    else {
        println!("\tCreated package \"{}\"", pkg_name);
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct RepoData {
    packages: HashMap<String, String>
}
impl RepoData {
    async fn get_from(json_url: &str) -> Result<RepoData> {
        let http = Client::new();
        let a: RepoData = http.get(json_url).send().await?.json().await?;
        Ok(a)
    }

    async fn install(&self, pkg_path: &Path, pkg_name: &str) -> Result<()> {
        let http = Client::new();
        let pkg_url = self.packages.get(pkg_name).ok_or(RepoError::NotFound(pkg_name.into()))?;
        let zip_data = http.get(pkg_url).send().await?.bytes().await?;
        extract_zip(zip_data, &pkg_path.join(pkg_name))
    }
}

fn extract_zip(zip: Bytes, out_path: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(zip))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_path = out_path.join(file.name());
        if file.name().ends_with("/") {
            fs::create_dir_all(file.name())?;
        }
        else {
            if let Some(p) = file_path.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
        }
        let mut outfile = fs::File::create(&file_path)?;
        io::copy(&mut file, &mut outfile )?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&file_path, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("Package \"{}\" not found on repository",.0)]
    NotFound(String)
}