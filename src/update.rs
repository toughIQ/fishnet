use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListBucket {
    contents: Vec<Contents>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Contents {
    key: String,
}

#[derive(Debug)]
enum UpdateError {
    Network(reqwest::Error),
    Xml(quick_xml::DeError),
}

impl From<reqwest::Error> for UpdateError {
    fn from(err: reqwest::Error) -> UpdateError {
        UpdateError::Network(err)
    }
}

impl From<quick_xml::DeError> for UpdateError {
    fn from(err: quick_xml::DeError) -> UpdateError {
        UpdateError::Xml(err)
    }
}

async fn maybe_update() -> Result<ListBucket, UpdateError> {
    let releases = quick_xml::de::from_str(
        &reqwest::get("https://fishnet-releases.s3.dualstack.eu-west-3.amazonaws.com/?list-type=2")
            .await?
            .text()
            .await?,
    )?;
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_update() {
        dbg!(maybe_update().await);
        panic!();
    }
}
