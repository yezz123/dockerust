#[derive(serde::Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct DockerBlobRef {
    pub mediaType: String,
    pub digest: String,
    pub size: Option<usize>,
}

#[allow(non_snake_case)]
#[derive(serde::Deserialize, Clone)]
pub struct DockerManifest {
    pub schemaVersion: usize,
    pub mediaType: String,
    pub config: DockerBlobRef,
    pub layers: Vec<DockerBlobRef>,
}

#[allow(non_snake_case)]
#[derive(serde::Deserialize, Clone)]
pub struct DockerManifestList {
    pub schemaVersion: usize,
    pub mediaType: String,
    pub manifests: Vec<DockerBlobRef>,
}

#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
pub struct DockerManifestOrManifestList {
    pub schemaVersion: usize,
    pub mediaType: String,
    pub config: Option<DockerBlobRef>,
    pub layers: Option<Vec<DockerBlobRef>>,
    pub manifests: Option<Vec<DockerBlobRef>>,
}

impl DockerManifestOrManifestList {
    pub fn get_manifest(&self) -> Option<DockerManifest> {
        if self
            .mediaType
            .eq("application/vnd.docker.distribution.manifest.v2+json")
            && self.config.is_some()
            && self.layers.is_some()
        {
            return Some(DockerManifest {
                schemaVersion: self.schemaVersion,
                mediaType: self.mediaType.to_string(),
                config: self.config.clone().unwrap(),
                layers: self.layers.clone().unwrap(),
            });
        }

        None
    }

    pub fn get_manifests_list(&self) -> Option<DockerManifestList> {
        if self
            .mediaType
            .eq("application/vnd.docker.distribution.manifest.list.v2+json")
            && self.manifests.is_some()
        {
            return Some(DockerManifestList {
                schemaVersion: self.schemaVersion,
                mediaType: self.mediaType.to_string(),
                manifests: self.manifests.clone().unwrap(),
            });
        }

        None
    }
}
