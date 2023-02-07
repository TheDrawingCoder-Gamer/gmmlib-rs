use std::collections::HashMap;
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct InstallData {
    pub version: String,
    pub structure: Vec<String>,
}
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct InstallDatas {
    pub mods: HashMap<String, InstallData>,
}
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct SavedState {
    pub gorilla_path: String,
    pub version_data: InstallDatas,
}

impl Default for InstallDatas {
    fn default() -> Self {
        Self {mods: HashMap::new()}
    }
}

pub fn mangle_name(name: &str) -> String {
    let ok_name = name.to_string();
    let chars: String = ok_name
        .chars()
        .map(|x| {
            if x.is_ascii_alphanumeric() {
                return x.to_ascii_lowercase() as char;
            } else {
                return '-' as char;
            }
        })
        .collect::<String>();
    return chars;
}

pub fn download(url: &str) -> std::io::Result<Vec<u8>> {
    let mut easy = curl::easy::Easy::new();
    easy.url(url)?;
    easy.follow_location(true)?;
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            buf.extend_from_slice(data);
            Ok(data.len())
        })
        .unwrap();
        transfer.perform()?;
    }
    Ok(buf)
}
pub fn fetch_mods(url: &str) -> std::io::Result<Vec<ModInfo>> {
    let buf = download(url)?; 
    let json_data = String::from_utf8(buf).unwrap();
    serde_json::from_str(&json_data).map_err(|it| std::io::Error::new(std::io::ErrorKind::Other, it))
}

pub fn fetch_groups(url: &str) -> std::io::Result<Vec<Group>> {
    let buf = download(url)?;
    serde_json::from_slice(&buf).map_err(|it| std::io::Error::new(std::io::ErrorKind::Other, it))
}

pub fn install_no_deps<F: Fn(&str) -> ()>(info: &ModInfo, path: &str, logger: F) -> std::io::Result<InstallData> {
    logger(format!("Installing {}...",info.name).as_str());
    let data = download(&info.download_url).unwrap();
    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader).unwrap();
    let mut da_path: String = path.to_string();
    if let Some(install_location) = &info.install_location {
        da_path = std::path::Path::new(&path)
            .join(install_location)
            .to_str()
            .unwrap()
            .to_string();
    }
    let structure: Vec<String> = archive.file_names().map(|it| it.to_string()).collect();
    archive.extract(da_path).unwrap();
    Ok(InstallData {structure, version: info.version.clone()})
}

pub fn install_mods<F: Fn(&str) -> ()>(
    infos: &Vec<ModInfo>,
    to_install: &Vec<ModInfo>,
    path: &str,
    logger: F,
    install_data: &InstallDatas,
    ignore_deps: bool
    ) -> std::io::Result<InstallDatas> {
    let closure: Box<dyn Fn(&ModInfo) -> bool> = Box::new(|it| {

        match install_data.mods.get(&it.name) {
            Some(v) => v.version != it.version,
            None => true
        }
    });
    let good_install: Vec<ModInfo> = to_install
        .clone()
        .iter()
        .filter(|it| closure(it))
        .map(|it| it.clone())
        .collect();
    let mut deps: Vec<ModInfo> = good_install
        .clone()
        .iter()
        .flat_map(|it| it.dependencies.clone())
        .flatten()
        .map(|it| infos.iter().find(|ti| it == ti.name))
        .flatten()
        .map(|it| it.clone())
        .filter(|it| closure(it))
        .collect();
    deps.sort_unstable_by_key(|it| it.name.clone());
    deps.dedup_by_key(|it| it.name.clone());
    let mut id = InstallDatas::default();
    if !ignore_deps {
        if !deps.is_empty() {
            id = install_mods(infos, &deps, path, &logger, install_data, false)?;
        }
    }
    for m in good_install {
        id.mods.insert(m.name.clone(), install_no_deps(&m, path, &logger)?);
    }
    Ok(install_data.merge(&id))
}
impl InstallDatas {
    /// Merges preffereing that over this. 
    pub fn merge(&self, that: &InstallDatas) -> InstallDatas {
        let mut mods: HashMap<String, InstallData> = HashMap::new();
        self.mods.iter().for_each(|(k, v)| {
            mods.insert(k.clone(), v.clone());
        });
        that.mods.iter().for_each(|(k, v)| {
            mods.insert(k.clone(), v.clone());
        });
        InstallDatas { mods }
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ModInfo {
    pub name: String,
    pub author: String,
    pub version: String,
    pub download_url: String,
    #[serde(default)]
    pub git_path: Option<String>,
    pub group: String,
    #[serde(default)]
    pub dependencies: Option<Vec<String>>,
    #[serde(default)]
    pub install_location: Option<String>,
    #[serde(default)]
    pub beta: bool,
}

pub fn get_mmm_mods() -> std::io::Result<Vec<ModInfo>> {
    fetch_mods(&"https://raw.githubusercontent.com/DeadlyKitten/MonkeModInfo/master/modinfo.json")
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
pub struct Group {
    pub rank: u8,
    pub name: String,
}

pub fn get_mmm_groups() -> std::io::Result<Vec<Group>> {
    fetch_groups(&"https://raw.githubusercontent.com/DeadlyKitten/MonkeModInfo/master/groupinfo.json")
}

#[derive(Clone, PartialEq, Eq)]
pub struct GroupBox {
    pub group: Group,
    pub mods: Vec<ModInfo>,
}
use itertools::Itertools;
pub fn grouped(groups: Vec<Group>, infos: Vec<ModInfo>) -> Vec<GroupBox> {
    let mut is = infos;
    is.sort_unstable_by_key(|it| it.group.clone());
    let mut data: Vec<GroupBox> = Itertools::group_by(is.iter(), |it| it.group.clone())
        .into_iter()
        .map(|(key, its)| {
            let group = groups.iter().find(|it| it.name == key).unwrap();
            GroupBox {group: group.clone(), mods: its.map(|i| i.clone()).collect() }
        })
        .collect();
    data.sort_by_key(|it| it.group.rank);
    data
}
