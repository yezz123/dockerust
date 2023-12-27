use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::docker::{DockerBlobRef, DockerManifest, DockerManifestOrManifestList};

const BASE_PATH: &str = "docker/registry/v2/";

#[derive(Debug, Eq, PartialEq)]
pub struct BlobReference {
    alg: String,
    hash: String,
}

impl BlobReference {
    pub fn from_docker_blob_ref(r: &DockerBlobRef) -> std::io::Result<Self> {
        Self::from_str(&r.digest)
    }

    pub fn from_sha256sum(hash: String) -> Self {
        Self {
            alg: "sha256".to_string(),
            hash,
        }
    }

    pub fn is_valid_reference(r: &str) -> bool {
        Self::from_str(r).is_ok()
    }

    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        Self::from_str(&std::fs::read_to_string(path)?)
    }

    pub fn to_digest(&self) -> String {
        format!("{}:{}", self.alg, self.hash)
    }

    pub fn data_path(&self, storage_path: &Path) -> PathBuf {
        storage_path
            .join(BASE_PATH)
            .join("blobs")
            .join(&self.alg)
            .join(&self.hash[..2])
            .join(&self.hash)
            .join("data")
    }

    pub fn is_empty_ref(&self) -> bool {
        self.alg == "sha256" && self.hash == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    }
}

impl FromStr for BlobReference {
    type Err = std::io::Error;

    fn from_str(content: &str) -> Result<Self, Self::Err> {
        let split = content.splitn(2, ':').collect::<Vec<_>>();

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
}

#[derive(Debug)]
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
        self.storage_path.join(BASE_PATH).join("repositories").join(&self.image)
    }

    pub fn tags_path(&self) -> PathBuf {
        self.image_path().join("_manifests/tags")
    }

    pub fn revisions_path(&self) -> PathBuf {
        self.image_path().join("_manifests/revisions")
    }

    pub fn tags_list(&self) -> std::io::Result<Vec<String>> {
        let mut list = vec![];
        if !self.tags_path().exists() {
            return Ok(vec![]);
        }

        for entry in std::fs::read_dir(self.tags_path())? {
            let entry = entry?;
            if entry.metadata()?.is_dir() {
                let manifest_tag = entry.file_name().to_string_lossy().to_string();

                // We check the link actually exists before adding it to the list
                if self.manifest_tag_link_path(&manifest_tag).exists() {
                    list.push(manifest_tag);
                }
            }
        }
        Ok(list)
    }

    pub fn get_tags_attached_to_manifest_blob(&self, b: &BlobReference) -> std::io::Result<Vec<String>> {
        let mut list = vec![];

        for tag in self.tags_list()? {
            let blob = BlobReference::from_file(&self.manifest_tag_link_path(&tag))?;

            if &blob == b {
                list.push(tag);
            }
        }

        Ok(list)
    }

    pub fn manifests_revision_list(&self) -> std::io::Result<Vec<BlobReference>> {
        let list_path = self.revisions_path().join("sha256");
        if !list_path.exists() {
            return Ok(vec![]);
        }

        let mut list = vec![];
        for entry in std::fs::read_dir(list_path)? {
            let entry = entry?;
            if entry.metadata()?.is_dir() {
                let link_file = entry.path().join("link");
                if link_file.exists() {
                    list.push(BlobReference::from_file(&link_file)?);
                }
            }
        }
        Ok(list)
    }

    pub fn manifest_tag_link_path(&self, manifest_ref: &str) -> PathBuf {
        self.tags_path().join(manifest_ref).join("current/link")
    }

    pub fn manifest_revision_path(&self, blob: &BlobReference) -> PathBuf {
        self.revisions_path().join(&blob.alg).join(&blob.hash).join("link")
    }

    pub fn upload_storage_path(&self, uuid: &str) -> PathBuf {
        self.image_path().join("_uploads").join(uuid)
    }
}

pub fn recurse_images_scan(path: &Path, start: &Path) -> std::io::Result<Vec<String>> {
    if !path.exists() || !path.is_dir() {
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

            return Ok(vec![image_path[start_path.len() + 1..].to_string()]);
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

/// Get the entire list of blob references
pub fn get_blob_list(storage: &Path) -> std::io::Result<Vec<BlobReference>> {
    let root = storage.join(BASE_PATH).join("blobs/sha256");
    let mut list = vec![];

    if !root.exists() {
        return Ok(list);
    }

    // First level parsing
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;

        if !entry.metadata()?.is_dir() {
            continue;
        }

        // Second level parsing
        for entry in std::fs::read_dir(entry.path())? {
            let entry = entry?;

            if entry.metadata()?.is_dir() {
                list.push(BlobReference::from_sha256sum(
                    entry.file_name().to_string_lossy().to_string(),
                ))
            }
        }
    }

    Ok(list)
}

fn is_blob_useless_in_docker_manifest(blob_ref: &BlobReference, manifest: &DockerManifest) -> std::io::Result<bool> {
    // Check config
    if &BlobReference::from_docker_blob_ref(&manifest.config)? == blob_ref {
        return Ok(false);
    }

    // Check layers
    for layer in &manifest.layers {
        if &BlobReference::from_docker_blob_ref(layer)? == blob_ref {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Check recursively manifest distribution files
fn is_blob_useless_in_distribution_file(
    blob_ref: &BlobReference,
    upper_manifest_ref: &BlobReference,
    storage: &Path,
) -> std::io::Result<bool> {
    let manifest_path = upper_manifest_ref.data_path(storage);

    if !manifest_path.exists() {
        return Ok(true);
    }

    let manifest: DockerManifestOrManifestList = serde_json::from_str(&std::fs::read_to_string(manifest_path)?)?;

    // In case of manifest file
    if let Some(manifest) = manifest.get_manifest() {
        if !is_blob_useless_in_docker_manifest(blob_ref, &manifest)? {
            return Ok(false);
        }
    }
    // In case of distribution files => recurse scan
    else if let Some(manifests_list) = manifest.get_manifests_list() {
        for manifest_ref in &manifests_list.manifests {
            let manifest_ref = BlobReference::from_docker_blob_ref(manifest_ref)?;

            if &manifest_ref == blob_ref {
                return Ok(false);
            }

            if &manifest_ref == upper_manifest_ref {
                continue;
            }

            if !is_blob_useless_in_distribution_file(blob_ref, &manifest_ref, storage)? {
                return Ok(false);
            }
        }
    } else {
        eprintln!("Unknown manifest type! {}", manifest.mediaType);
    }

    Ok(true)
}

/// Check if a blob is useless or not
pub fn is_blob_useless(blob_ref: &BlobReference, storage: &Path) -> std::io::Result<bool> {
    // Scan all images
    for image in get_docker_images_list(storage)? {
        let image = DockerImage::new(storage, &image);

        let mut manifest_blobs = image.manifests_revision_list()?;

        // Process each image tags
        for tag in image.tags_list()? {
            let manifest_ref = BlobReference::from_file(&image.manifest_tag_link_path(&tag))?;

            if !manifest_ref.is_empty_ref() {
                manifest_blobs.push(manifest_ref);
            }
        }

        for manifest_ref in manifest_blobs {
            if &manifest_ref == blob_ref {
                return Ok(false);
            }

            if !is_blob_useless_in_distribution_file(blob_ref, &manifest_ref, storage)? {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Remove empty directories
fn remove_empty_dirs(path: &Path, can_remove: bool) -> std::io::Result<()> {
    let mut found_files = false;

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;

        found_files = true;

        if entry.metadata()?.is_dir() {
            remove_empty_dirs(&entry.path(), true)?;
        }
    }

    if !found_files && can_remove {
        std::fs::remove_dir(path)?;
    }

    Ok(())
}

/// Run the garbage collector
pub fn clean_storage(storage: &Path) -> std::io::Result<()> {
    for _ in 0..3 {
        for blob in get_blob_list(storage)? {
            // Empty blob
            if blob.is_empty_ref() {
                continue;
            }

            if !is_blob_useless(&blob, storage)? {
                continue;
            }

            println!("Deleting useless blob {}", blob.to_digest());
            std::fs::remove_dir_all(blob.data_path(storage).parent().unwrap())?;
        }

        remove_empty_dirs(storage, false)?;
    }

    Ok(())
}
