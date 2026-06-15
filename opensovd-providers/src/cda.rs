use opensovd_core::Component;
use opensovd_core::DiscoveryProvider;

pub struct CdaProvider {
    host: String,
    port: u16,
    base_path: String,
    token: String,
    client: reqwest::Client,
}

impl CdaProvider {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        base_path: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            base_path: base_path.into(),
            token: token.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Asynchronously fetches data from the CDA via REST/HTTP.
    ///
    /// # Errors
    ///
    /// Returns a `reqwest::Error` if the HTTP request fails or the response body cannot be read.
    pub async fn fetch_cda_path(
        &self,
        path: &str,
    ) -> Result<(reqwest::StatusCode, String), reqwest::Error> {
        let url = format!("http://{}:{}{}", self.host, self.port, path);

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        Ok((status, body))
    }
}

/// Data provider for CDA components.
pub struct CdaDataProvider {
    component_id: String,
    cda_host: String,
    cda_port: u16,
    base_path: String,
    token: String,
    /// Stores (ID, Name) tuples of available data points.
    data_points: Vec<(String, String)>,
    client: reqwest::Client,
}

#[async_trait::async_trait]
impl opensovd_core::DataProvider for CdaDataProvider {
    async fn list(
        &self,
        _filter: opensovd_core::DataFilter,
    ) -> Result<Vec<opensovd_core::Metadata>, opensovd_core::DataError> {
        let items = self
            .data_points
            .iter()
            .map(|(id, name)| opensovd_core::Metadata {
                id: id.clone(),
                name: name.clone(),
                category: "cda-data".to_string(),
                translation_id: None,
                groups: vec![],
                tags: vec![],
                schema: None,
                is_readable: true,
                is_writable: true,
            })
            .collect();
        Ok(items)
    }

    async fn read(
        &self,
        data_id: &str,
        _include_schema: bool,
    ) -> Result<opensovd_core::Data, opensovd_core::DataError> {
        let url = format!(
            "http://{}:{}{}/components/{}/data/{}",
            self.cda_host, self.cda_port, self.base_path, self.component_id, data_id
        );

        let response = match self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
        {
            Ok(res) => res,
            Err(e) => {
                return Ok(opensovd_core::Data {
                    data: serde_json::json!({ "error": format!("CDA nicht erreichbar: {}", e) }),
                    schema: None,
                });
            }
        };

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            let error_details =
                serde_json::from_str::<serde_json::Value>(&body).unwrap_or(serde_json::json!(body));
            return Ok(opensovd_core::Data {
                data: serde_json::json!({
                    "error": format!("CDA lieferte HTTP Status: {}", status),
                    "cda_message": error_details
                }),
                schema: None,
            });
        }

        let mut data_value = serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or(serde_json::json!({ "raw": body }));

        // Unwrap nested "data" field if present (CDA specific behavior).
        if let Some(inner_data) = data_value.get("data") {
            data_value = inner_data.clone();
        }

        Ok(opensovd_core::Data {
            data: data_value,
            schema: None,
        })
    }

    async fn write(
        &self,
        data_id: &str,
        value: serde_json::Value,
    ) -> Result<(), opensovd_core::DataError> {
        let url = format!(
            "http://{}:{}{}/components/{}/data/{}",
            self.cda_host, self.cda_port, self.base_path, self.component_id, data_id
        );

        // Wrap payload in a "data" object required by the CDA.
        let payload = serde_json::json!({
            "data": value
        });

        let response = match self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(payload.to_string())
            .send()
            .await
        {
            Ok(res) => res,
            Err(e) => {
                return Err(opensovd_core::DataError::Internal(format!(
                    "CDA nicht erreichbar: {e}"
                )));
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(opensovd_core::DataError::Internal(format!(
                "CDA HTTP Status {status}: {body}"
            )));
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for CdaProvider {
    #[allow(clippy::too_many_lines)]
    async fn discover(
        &self,
    ) -> Result<
        std::pin::Pin<
            std::boxed::Box<
                dyn futures::stream::Stream<
                        Item = Result<
                            (
                                Vec<opensovd_core::EntityRef>,
                                opensovd_core::EntityCollection,
                            ),
                            opensovd_core::DiscoveryError,
                        >,
                    > + Send
                    + 'static,
            >,
        >,
        opensovd_core::DiscoveryError,
    > {
        let mut collection = opensovd_core::EntityCollection::default();

        let components_path = format!("{}/components", self.base_path);
        match self.fetch_cda_path(&components_path).await {
            Ok((status, text)) => {
                tracing::info!(
                    "Received REST response! Status: {status}, Length: {}",
                    text.len()
                );

                if text.is_empty() {
                    tracing::warn!("Response body is empty. Check the URL or CDA status.");
                } else {
                    match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(json) => {
                            tracing::info!("Successfully parsed JSON from CDA:\n{json:#?}");

                            let items_array = json.get("items").and_then(|v| v.as_array()).or_else(|| json.as_array());
                            if let Some(items) = items_array {
                                if items.is_empty() {
                                    tracing::info!(
                                        "Info: The 'items' list from CDA is currently empty."
                                    );
                                } else {
                                    for (index, item) in items.iter().enumerate() {
                                        tracing::info!("- Processing item {index}: {item}");

                                        let id = item
                                            .get("id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown_id");
                                        let name = item
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Unknown Component");

                                        let mut cda_component = Component::new(id, name);
                                        let mut found_data = Vec::new();

                                        // Fetch available data points for the component.
                                        let data_path =
                                            format!("{}/components/{id}/data", self.base_path);
                                        tracing::info!("  -> Fetching data from: {data_path}");
                                        match self.fetch_cda_path(&data_path).await {
                                            Ok((data_status, data_text)) => {
                                                if data_status.is_success() && !data_text.is_empty()
                                                {
                                                    if let Ok(data_json) =
                                                        serde_json::from_str::<serde_json::Value>(
                                                            &data_text,
                                                        )
                                                    {
                                                        let data_items_array = data_json.get("items")
                                                            .and_then(|v| v.as_array())
                                                            .or_else(|| data_json.as_array());

                                                        if let Some(data_items) = data_items_array {
                                                            for data_item in data_items {
                                                                let data_id = data_item
                                                                    .get("id")
                                                                    .and_then(|v| v.as_str())
                                                                    .unwrap_or("unknown_data");
                                                                let data_name = data_item
                                                                    .get("name")
                                                                    .and_then(|v| v.as_str())
                                                                    .unwrap_or("Unknown Data");

                                                                found_data.push((
                                                                    data_id.to_string(),
                                                                    data_name.to_string(),
                                                                ));
                                                                tracing::info!(
                                                                    "     Discovered data point: {data_id} - {data_name}"
                                                                );
                                                            }
                                                        }
                                                    } else {
                                                        tracing::warn!(
                                                            "     Failed to parse JSON data for {id}."
                                                        );
                                                    }
                                                } else {
                                                    tracing::info!(
                                                        "    Info: No data found for component {id} (Status: {data_status})"
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "     ERROR fetching data for {id}: {e}"
                                                );
                                            }
                                        }

                                        let provider = CdaDataProvider {
                                            component_id: id.to_string(),
                                            cda_host: self.host.clone(),
                                            cda_port: self.port,
                                            base_path: self.base_path.clone(),
                                            token: self.token.clone(),
                                            data_points: found_data,
                                        client: self.client.clone(),
                                        };
                                        cda_component = cda_component.with_data_provider(provider);

                                        collection.components.push(cda_component);
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::warn!("Failed to parse JSON from CDA: {e}"),
                    }
                }
            }
            Err(e) => tracing::error!("ERROR during REST connection: {e}"),
        }

        let stream = futures::stream::once(std::future::ready(Ok((vec![], collection))));
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use opensovd_core::DataProvider;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_cda_provider_fetch_path() {
        // 1. Start a local mock server
        let mock_server = MockServer::start().await;

        // 2. Configure expected behavior of the "CDA server"
        Mock::given(method("GET"))
            .and(path("/vehicle/v15/components"))
            .and(header("Authorization", "Bearer my-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"status\":\"ok\"}"))
            .mount(&mock_server)
            .await;

        // Parse host and port dynamically from the mock server
        let uri = mock_server.uri();
        let url = url::Url::parse(&uri).unwrap();
        let host = url.host_str().unwrap().to_string();
        let port = url.port().unwrap();

        // 3. Initialize the real provider with the mock data
        let provider = CdaProvider::new(host, port, "/vehicle/v15", "my-token");
        let (status, body) = provider.fetch_cda_path("/vehicle/v15/components").await.unwrap();

        // 4. Verify the results
        assert!(status.is_success());
        assert_eq!(body, "{\"status\":\"ok\"}");
    }

    #[tokio::test]
    async fn test_cda_data_provider_read_success() {
        let mock_server = MockServer::start().await;

        // The real CDA server often delivers values wrapped in a "data" object
        let mock_response = json!({
            "data": { "vin": "WBA0000000000" }
        });

        Mock::given(method("GET"))
            .and(path("/vehicle/v15/components/flxc1000/data/VINDataIdentifier"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
            .mount(&mock_server)
            .await;

        let uri = mock_server.uri();
        let url = url::Url::parse(&uri).unwrap();
        
        let provider = CdaDataProvider {
            component_id: "flxc1000".to_string(),
            cda_host: url.host_str().unwrap().to_string(),
            cda_port: url.port().unwrap(),
            base_path: "/vehicle/v15".to_string(),
            token: "test-token".to_string(),
            data_points: vec![],
            client: reqwest::Client::new(),
        };

        let result = provider.read("VINDataIdentifier", false).await.unwrap();
        
        // Check if our logic successfully removed the outer "data" field
        assert_eq!(result.data, json!({ "vin": "WBA0000000000" }));
    }

    #[tokio::test]
    async fn test_cda_data_provider_write() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/vehicle/v15/components/flxc1000/data/SomeIdentifier"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let uri = mock_server.uri();
        let url = url::Url::parse(&uri).unwrap();
        
        let provider = CdaDataProvider {
            component_id: "flxc1000".to_string(),
            cda_host: url.host_str().unwrap().to_string(),
            cda_port: url.port().unwrap(),
            base_path: "/vehicle/v15".to_string(),
            token: "test-token".to_string(),
            data_points: vec![],
            client: reqwest::Client::new(),
        };

        // Call the `write` method
        let result = provider.write("SomeIdentifier", json!("new_value")).await;
        assert!(result.is_ok());
    }
}
