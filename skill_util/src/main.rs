use std::collections::HashMap;
use std::env::current_dir;
use std::fs::{self, DirEntry, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::Path;

use anyhow::{anyhow, Result};
use bytes::Bytes;
use clap::{Arg, App, AppSettings, SubCommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

const DEF_REPO_URL: &str = "http://lily-skills.s3-website.eu-west-3.amazonaws.com/list.json";
const SKILLS_PATH: &str = "../skills";
const CURR_LILY_VER: (u8, u8) = (0,6);

#[tokio::main(flavor="current_thread")]
async fn main() -> anyhow::Result<()> {
    let matches = App::new("Skill utility")
                  .version("0.3")
                  .setting(AppSettings::SubcommandRequiredElseHelp)
                  .subcommand(SubCommand::with_name("init")
                              .arg(
                                  Arg::with_name("skill_name").required(true)
                                )
                            )
                   .subcommand(SubCommand::with_name("install")
                              .arg(
                                  Arg::with_name("skill_name").required(true)
                                )
                            )
                    .subcommand(SubCommand::with_name("remove")
                               .arg(
                                    Arg::with_name("skill_name").required(true)
                               )
                            )
                    .subcommand(SubCommand::with_name("repo")
                                .subcommand(SubCommand::with_name("init")
                                    .arg(
                                        Arg::with_name("repo_path").required(true)
                                    )
                                )
                                .subcommand(SubCommand::with_name("add_skill")
                                    .arg(
                                        Arg::with_name("skill_path").required(true)
                                    )
                                    .arg(
                                        Arg::with_name("repo_path").required(true)
                                    )
                                )
                            )
                  .get_matches();

    if let Some(init_data) = matches.subcommand_matches("init") {
        do_init(init_data.value_of("skill_name").expect("Couldn't get skill_name but is required... Wut?"))?;
    }
    else if let Some(install_data) = matches.subcommand_matches("install") {
        let repo = RepoData::get_from(DEF_REPO_URL).await?;
        if let Err(e) = repo.install(Path::new(SKILLS_PATH),install_data.value_of("skill_name").expect("Couldn't get skill_name but is required... Wut?")).await {
            match e.downcast::<RepoError>() {
                Ok(e) => {eprintln!("{}", e)},
                Err(e) => {Err(e)?}
            }
        }
    }
    else if let Some(remove_data) = matches.subcommand_matches("remove") {
        let path = Path::new(SKILLS_PATH);
        let skill_name = remove_data.value_of("skill_name").expect("Couldn't get skill_name but is required... Wut?");
        let pkg_path = path.join(skill_name);
        if let Err(e) = fs::remove_dir_all(pkg_path) {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Skill \"{}\" does not exist", skill_name);
            }
            else {Err(e)?}
        }
    }
    else if let Some(repo_data) = matches.subcommand_matches("repo") {
        if let Some(prep_data) = repo_data.subcommand_matches("init") {
            let repo_path = Path::new(prep_data.value_of("repo_path").expect("Couldn't get repo path"));
            if !repo_path.exists() {
                fs::create_dir_all(repo_path)?;
            }

            let skills_path = Path::new(SKILLS_PATH);
            let mut repo_data = RepoData::empty();
            for child in skills_path.read_dir()? {
                let child = child?.path();
                if child.is_dir() {
                    repo_data.zip_and_add_internal(&child, repo_path)?;
                }
            }
            
            // list.json
            let writer = File::create(repo_path.join("list.json"))?;
            serde_json::to_writer(writer, &repo_data)?;

            // err404.json
            const MSG_404: &str = "{\"error\":\"404\",\"message\":\"Resource not found\"}";
            fs::write(repo_path.join("err404.json"), MSG_404)?;
        }
        else if let Some(add_data) = repo_data.subcommand_matches("add_skill") {
            let repo_path = Path::new(add_data.value_of("repo_path").expect("Couldn't get repo path"));
            let skill_path = Path::new(add_data.value_of("skill_path").expect("Couldn't get repo path"));
            assert!(repo_path.exists());
            assert!(skill_path.exists());
            
            let list_path = repo_path.join("list.json");
            let list = File::open(&list_path).expect("No list.json found");
            let mut data: RepoData = serde_json::from_reader(io::BufReader::new(list)).expect("Failed to decode");            
            data.zip_and_add_internal(skill_path, repo_path)?;
            let list = OpenOptions::new().write(true).open(list_path)?;
            serde_json::to_writer(io::BufWriter::new(list), &data).unwrap();
        }
    }

    Ok(())
}

fn first_upper(mut input: String) -> String {
    let tail = input.split_off(1);
    let mut head = input.to_uppercase();
    head.push_str(&tail);
    head
}

fn do_init(pkg_path_str: &str) -> Result<()> {
    // Create main folder
    let pkg_path = Path::new(pkg_path_str);
    let skill_name = pkg_path
                    .file_name().ok_or_else(||anyhow!("Path must follow in some name"))?
                    .to_str().ok_or_else(||anyhow!("Skill name must be valid Unicode"))?;
    

    // If the folder exists, stop right on the track
    if pkg_path.exists() {
        return Err(anyhow!("Skill already exists"))
    }

    fs::create_dir_all(&pkg_path)?;

    // Create Python module
    let py_mod_path = pkg_path.join("python").join(skill_name);
    fs::create_dir_all(&py_mod_path)?;
    let mut py_file = fs::File::create(py_mod_path.join("__init__.py"))?;
    write!(&mut py_file, "from lily_ext import action, answer, conf, translate

@action(name = \"default_action\")
class {}:

    def __init__(self):
        pass

    def trigger_action(self, context):
        if context[\"intent\"] == \"example\":
            return answer(translate(\"example_translation_say\",context), context)
    ", first_upper(skill_name.to_lowercase()))?;

    // Create translation
    let trans_path = pkg_path.join("translations");
    fs::create_dir(&trans_path)?;

    let translations = &[
        ("en-US", "example_translation_say = Hello there, ($friend_name)
        ")
    ];
    for (trans_lang, trans_demo) in translations {
        let lang_path = trans_path.join(trans_lang);
        fs::create_dir(&lang_path)?;

        let mut trans_file = fs::File::create(lang_path.join("main.ftl"))?;
        write!(&mut trans_file, "{}", trans_demo)?;
    }

    // Skills definition file
    let mut model = fs::File::create(pkg_path.join("model.yaml"))?;
    write!(&mut model, "example:
    samples: \"Say hello to ($friend_name)\"
    slots:
        friend_name:
            type:
                data:
                    - Alex
                    - John
    action: default_action
")?;

    print_path_cute(skill_name,&pkg_path)?;

    Ok(())
}

fn print_path_cute(skill_name: &str, pkg_path: &Path) -> Result<()> {
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
            println!("\tCreated skill \"{}\" at \"{}\"", skill_name, parent_path.canonicalize()?.to_string_lossy());
        }
        else {
            println!("\tCreated skill \"{}\"", skill_name);
        }    
    }
    else {
        println!("\tCreated skill \"{}\"", skill_name);
    }

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
enum SkillUrl {
    #[serde(rename="internal")]
    Internal(String),
    #[serde(rename="external")]
    External(String)
}

#[derive(Debug, Deserialize, Serialize)]
struct SkillData {
    url: SkillUrl,
    min_ver: (u8,u8)
}

impl SkillData {
    fn new_internal(path: String, min_ver: (u8, u8)) -> Self {
        SkillData{url: SkillUrl::Internal(path), min_ver}
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct RepoData {
    skills: HashMap<String, SkillData>,
    #[serde(skip, default="empty_url")]
    repo_url: Option<Url>
}
fn empty_url() -> Option<Url> {
    None
}
impl RepoData {
    fn empty() -> Self {
        Self{skills: HashMap::new(), repo_url: empty_url()}
    }

    async fn get_from(json_url: &str) -> Result<RepoData> {
        let http = Client::new();
        let mut url = Url::parse(json_url)?;
        let mut res: RepoData = http.get(url.clone()).send().await?.json().await?;
        url.set_path("/");
        res.repo_url = Some(url);
        Ok(res)
    }

    async fn install(&self, skls_path: &Path, skill_name: &str) -> Result<()> {
        let http = Client::new();
        let skill_data = self.skills.get(skill_name).ok_or(RepoError::NotFound(skill_name.into()))?;
        if Self::is_ver_compatible(skill_data.min_ver) {
            let skl_url = match &skill_data.url {
                SkillUrl::Internal(repo_ref) => {
                    let mut url = self.repo_url.clone().expect("Can't perform install in a remote repo");
                    url.set_path(repo_ref);
                    url
                }
                SkillUrl::External(url) => Url::parse(url)?
            };
            let zip_data = http.get(skl_url).send().await?.bytes().await?;
            extract_zip(zip_data, &skls_path.join(skill_name))
        }
        else {
            let v = skill_data.min_ver;
            Err(anyhow!("This version requires a newer Lily: {}.{}", v.0, v.1))
        }
    }

    fn add_internal(&mut self, skill_name: &str, path:String) {
        let skl = SkillData::new_internal(path, CURR_LILY_VER);
        self.skills.insert(skill_name.to_string(), skl);
    }

    fn zip_and_add_internal(&mut self, skill_path: &Path, repo_path: &Path) -> Result<()> {
        let skill_name = skill_path.file_name().expect("Couldn't obtain folder name").to_str().expect("Can't transform os-str into str");
        let zip_name = format!("{}.zip", skill_name);
        let zip_path = repo_path.join(zip_name);
        zip_folder(skill_path, &zip_path)?;

        let child_path = zip_path.strip_prefix(repo_path)?;
        self.add_internal(skill_name, child_path.to_string_lossy().to_string());

        Ok(())
    }

    fn is_ver_compatible(ver: (u8, u8)) -> bool {
        CURR_LILY_VER.0 >= ver.0 && CURR_LILY_VER.1 >= ver.1
    }
}

fn extract_zip(zip: Bytes, out_path: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(zip))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_path = out_path.join(file.name());
        if file.name().ends_with("/") { // Is a directory
            fs::create_dir_all(&file_path)?;
        }
        else { // Is a file
            if let Some(p) = file_path.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = fs::File::create(&file_path)?;
            io::copy(&mut file, &mut outfile )?;
        }

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

fn zip_folder(folder_path: &Path, out_zip: &Path) -> Result<()> {
    let a = folder_path.read_dir()?;
    let writer = File::create(out_zip)?;
    let mut zip = ZipWriter::new(writer);
    zip_folder_impl(&mut a.filter_map(|e| e.ok()), folder_path, &mut zip)?;
    zip.finish()?;
    Ok(())
}
fn zip_folder_impl<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    prefix: &Path,
    zip: &mut ZipWriter<T>,
) -> Result<()>
where
    T: Write + Seek,
{
    let options = FileOptions::default()
        .compression_method(CompressionMethod::DEFLATE);
    const IGNORED: &str = "__pycache__";

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(prefix).unwrap().to_string_lossy();
        let is_ignored = path.file_name().map(|n|n==IGNORED).unwrap_or(true);

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if !is_ignored {
            if path.is_file() {
                zip.start_file(name, options)?;
                let mut f = File::open(path)?;

                f.read_to_end(&mut buffer)?;
                zip.write_all(&*buffer)?;
                buffer.clear();
            } else if name.len() != 0 && name != IGNORED {
                // Only if not root! Avoids path spec / warning
                // and mapname conversion failed error on unzip
                zip.add_directory(name, options)?;
                let it = path.read_dir()?;
                zip_folder_impl(&mut it.filter_map(|e|e.ok()), prefix, zip)?;
            }
        }
    }
    Result::Ok(())
}

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("Skill \"{}\" not found on repository",.0)]
    NotFound(String)
}