use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const BASE_PATH: &str = "docker/registry/v2/";

pub struct BlobReference {
    alg: String,
    hash: String,
}

impl BlobReference {
    pub fn from_str(content: &str) -> std::io::Result<Self> {
        let split = content.splitn(2, ":").collect::<Vec<_>>();

        if split.len() != 2 {
            return Err(std::io::Error::new(ErrorKind::Other, "Expected 2 entries!"));
        }

        if split[1].len() <= 2 {
            return Err(std::io::Error::new(ErrorKind::Other, "Blob hash is too small!"));
        }

        Ok(Self {
            alg: split[0].to_string(),
            hash: split[1].to_string(),
        })
    }

    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        Self::from_str(&std::fs::read_to_string(path)?)
    }

    pub fn to_digest(&self) -> String {
        format!("{}:{}", self.alg, self.hash)
    }

    pub fn data_path(&self, storage_path: &Path, blob: &BlobReference) -> PathBuf {
        storage_path
            .join(BASE_PATH)
            .join("blobs")
            .join(&blob.alg)
            .join(&blob.hash[..2])
            .join(&blob.hash)
            .join("data")
    }
}

pub struct DockerImage {
    pub storage_path: PathBuf,
    pub image: String,
}

impl DockerImage {
    pub fn new(storage: &Path, image: &str) -> Self {
        Self {
            storage_path: storage.to_path_buf(),
            image: image.to_string(),
        }
    }

    pub fn image_path(&self) -> PathBuf {
        self.storage_path
            .join(BASE_PATH)
            .join("repositories")
            .join(&self.image)
    }

    pub fn tags_path(&self) -> PathBuf {
        self.image_path()
            .join("_manifests/tags")
    }

    pub fn manifest_link_path(&self, manifest_ref: &str) -> PathBuf {
        self.tags_path()
            .join(manifest_ref)
            .join("current/link")
    }
}

pub fn recurse_images_scan(path: &Path, start: &Path) -> std::io::Result<Vec<String>> {
    if !path.is_dir() {
        return Ok(vec![]);
    }

    let mut list = vec![];

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        if entry.file_name().eq("_manifests") {
            let image_path = path.to_string_lossy().to_string();
            let start_path = start.to_string_lossy().to_string();

            return Ok(vec![
                image_path[start_path.len() + 1..].to_string()
            ]);
        } else {
            list.append(&mut recurse_images_scan(&entry.path(), start)?);
        }
    }

    Ok(list)
}

/// Get the entire list of docker image available
pub fn get_docker_images_list(storage: &Path) -> std::io::Result<Vec<String>> {
    let start = storage.join(BASE_PATH).join("repositories");
    let mut list = recurse_images_scan(&start, &start)?;
    list.sort();
    Ok(list)
}
