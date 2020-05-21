use super::error::{
    ConductorApiError, ConductorApiResult, ExternalApiWireError, SerializationError,
};
use crate::core::ribosome::{ZomeCallInvocation, ZomeCallInvocationResponse};
use crate::{
    conductor::{
        config::AdminInterfaceConfig,
        error::CreateAppError,
        interface::error::{InterfaceError, InterfaceResult},
        ConductorHandle,
    },
    core::workflow::ZomeCallInvocationResult,
};
use holo_hash::*;
use holochain_serialized_bytes::prelude::*;
use holochain_types::{
    app::{AppId, AppPaths, MembraneProofs},
    cell::{CellHandle, CellId},
    dna::{DnaFile, Properties},
};
use std::path::PathBuf;
use tracing::*;

/// A trait that unifies both the admin and app interfaces
#[async_trait::async_trait]
pub trait InterfaceApi: 'static + Send + Sync + Clone {
    /// Which request is being made
    type ApiRequest: TryFrom<SerializedBytes, Error = SerializedBytesError> + Send + Sync;
    /// Which response is sent to the above request
    type ApiResponse: TryInto<SerializedBytes, Error = SerializedBytesError> + Send + Sync;
    /// Handle a request on this API
    async fn handle_request(
        &self,
        request: Result<Self::ApiRequest, SerializedBytesError>,
    ) -> InterfaceResult<Self::ApiResponse>;
}

/// A trait for the interface that a Conductor exposes to the outside world to use for administering the conductor.
/// This trait has a one mock implementation and one "Real" implementation
#[async_trait::async_trait]
pub trait AdminInterfaceApi: 'static + Send + Sync + Clone {
    /// Call an admin function to modify this Conductor's behavior
    async fn handle_admin_request_inner(
        &self,
        request: AdminRequest,
    ) -> ConductorApiResult<AdminResponse>;

    // -- provided -- //

    /// Route the request to be handled
    async fn handle_admin_request(&self, request: AdminRequest) -> AdminResponse {
        let res = self.handle_admin_request_inner(request).await;

        match res {
            Ok(response) => response,
            Err(e) => AdminResponse::Error(e.into()),
        }
    }
}
/// The interface that a Conductor exposes to the outside world.
#[async_trait::async_trait]
pub trait AppInterfaceApi: 'static + Send + Sync + Clone {
    /// Invoke a zome function on any cell in this conductor.
    async fn call_zome(
        &self,
        invocation: ZomeCallInvocation,
    ) -> ConductorApiResult<ZomeCallInvocationResult>;

    // -- provided -- //

    /// Routes the [AppRequest] to the [AppResponse]
    async fn handle_request(&self, request: AppRequest) -> AppResponse {
        let res: ConductorApiResult<AppResponse> = async move {
            match request {
                AppRequest::ZomeCallInvocationRequest { request } => {
                    match self.call_zome(*request).await? {
                        Ok(response) => Ok(AppResponse::ZomeCallInvocationResponse {
                            response: Box::new(response),
                        }),
                        Err(e) => Ok(AppResponse::Error(e.into())),
                    }
                }
                _ => unimplemented!(),
            }
        }
        .await;

        match res {
            Ok(response) => response,
            Err(e) => AppResponse::Error(e.into()),
        }
    }
}

/// The admin interface that external connections
/// can use to make requests to the conductor
/// The concrete (non-mock) implementation of the AdminInterfaceApi
#[derive(Clone)]
pub struct RealAdminInterfaceApi {
    /// Mutable access to the Conductor
    conductor_handle: ConductorHandle,

    /// Needed to spawn an App interface
    // TODO: is this needed? it's not currently being used.
    app_api: RealAppInterfaceApi,
}

impl RealAdminInterfaceApi {
    pub(crate) fn new(conductor_handle: ConductorHandle) -> Self {
        let app_api = RealAppInterfaceApi::new(conductor_handle.clone());
        RealAdminInterfaceApi {
            conductor_handle,
            app_api,
        }
    }
}

#[async_trait::async_trait]
impl AdminInterfaceApi for RealAdminInterfaceApi {
    async fn handle_admin_request_inner(
        &self,
        request: AdminRequest,
    ) -> ConductorApiResult<AdminResponse> {
        use AdminRequest::*;
        match request {
            Start(_cell_handle) => unimplemented!(),
            Stop(_cell_handle) => unimplemented!(),
            AddAdminInterfaces(configs) => Ok(AdminResponse::AdminInterfacesAdded(
                self.conductor_handle
                    .clone()
                    .add_admin_interfaces(configs)
                    .await?,
            )),
            InstallApp { app_paths, proofs } => {
                trace!(?app_paths.dnas);
                let AppPaths {
                    app_id,
                    agent_key,
                    dnas,
                } = app_paths;
                let proofs = proofs.proofs;

                // Install Dnas
                let install_dna_tasks = dnas.into_iter().map(|(dna_path, properties)| async {
                    let dna = read_parse_dna(dna_path, properties).await?;
                    let hash = dna.dna_hash().clone();
                    self.conductor_handle.install_dna(dna).await?;
                    ConductorApiResult::Ok(hash)
                });

                // Join all the install tasks
                let cell_ids_with_proofs = futures::future::join_all(install_dna_tasks)
                    .await
                    .into_iter()
                    // If they are ok create proofs
                    .map(|result| {
                        result.map(|hash| {
                            (
                                CellId::from((hash.clone(), agent_key.clone())),
                                proofs.get(&hash).cloned(),
                            )
                        })
                    })
                    // Check all passed and return the poofs
                    .collect::<Result<Vec<_>, _>>()?;

                // Call genesis
                self.conductor_handle
                    .clone()
                    .genesis_cells(app_id, cell_ids_with_proofs)
                    .await?;

                Ok(AdminResponse::AppInstalled)
            }
            ListDnas => {
                let dna_list = self.conductor_handle.list_dnas().await?;
                Ok(AdminResponse::ListDnas(dna_list))
            }
            GenerateAgentPubKey => {
                let agent_pub_key = self
                    .conductor_handle
                    .keystore()
                    .clone()
                    .generate_sign_keypair_from_pure_entropy()
                    .await?;
                Ok(AdminResponse::GenerateAgentPubKey(agent_pub_key))
            }
            ListAgentPubKeys => {
                let pub_key_list = self
                    .conductor_handle
                    .keystore()
                    .clone()
                    .list_sign_keys()
                    .await?;
                Ok(AdminResponse::ListAgentPubKeys(pub_key_list))
            }
            ActivateApp { app_id } => {
                // Activate app
                self.conductor_handle.activate_app(app_id.clone()).await?;

                // Create cells
                let errors = self.conductor_handle.clone().setup_cells().await?;

                // Check if this app was created successfully
                errors
                    .into_iter()
                    // We only care about this app for the activate command
                    .find(|cell_error| match cell_error {
                        CreateAppError::Failed {
                            app_id: error_app_id,
                            ..
                        } => error_app_id == &app_id,
                    })
                    // There was an error in this app so return it
                    .map(|this_app_error| Ok(AdminResponse::Error(this_app_error.into())))
                    // No error, return success
                    .unwrap_or(Ok(AdminResponse::AppActivated))
            }
            DeactivateApp { app_id } => {
                // Activate app
                self.conductor_handle.deactivate_app(app_id.clone()).await?;
                Ok(AdminResponse::AppDeactivated)
            }
            AttachAppInterface { port } => {
                let port = port.unwrap_or(0);
                let port = self
                    .conductor_handle
                    .clone()
                    .add_app_interface(port)
                    .await?;
                Ok(AdminResponse::AppInterfaceAttached { port })
            }
            DumpState(cell_id) => {
                let state = self.conductor_handle.dump_cell_state(&cell_id).await?;
                Ok(AdminResponse::JsonState(state))
            }
        }
    }
}

/// Reads the [Dna] from disk and parses to [SerializedBytes]
async fn read_parse_dna(
    dna_path: PathBuf,
    properties: Option<serde_json::Value>,
) -> ConductorApiResult<DnaFile> {
    let dna_content = tokio::fs::read(dna_path)
        .await
        .map_err(|e| ConductorApiError::DnaReadError(format!("{:?}", e)))?;
    let mut dna = DnaFile::from_file_content(&dna_content).await?;
    if let Some(properties) = properties {
        let properties = SerializedBytes::try_from(Properties::new(properties))
            .map_err(SerializationError::from)?;
        dna = dna.with_properties(properties).await?;
    }
    Ok(dna)
}

#[async_trait::async_trait]
impl InterfaceApi for RealAdminInterfaceApi {
    type ApiRequest = AdminRequest;
    type ApiResponse = AdminResponse;

    async fn handle_request(
        &self,
        request: Result<Self::ApiRequest, SerializedBytesError>,
    ) -> InterfaceResult<Self::ApiResponse> {
        // Don't hold the read across both awaits
        {
            self.conductor_handle
                .check_running()
                .await
                .map_err(InterfaceError::RequestHandler)?;
        }
        match request {
            Ok(request) => Ok(AdminInterfaceApi::handle_admin_request(self, request).await),
            Err(e) => Ok(AdminResponse::Error(SerializationError::from(e).into())),
        }
    }
}

/// The Conductor lives inside an Arc<RwLock<_>> which is shared with all
/// other Api references
#[derive(Clone)]
pub struct RealAppInterfaceApi {
    conductor_handle: ConductorHandle,
}

impl RealAppInterfaceApi {
    /// Create a new instance from a shared Conductor reference
    pub fn new(conductor_handle: ConductorHandle) -> Self {
        Self { conductor_handle }
    }
}

#[async_trait::async_trait]
impl AppInterfaceApi for RealAppInterfaceApi {
    async fn call_zome(
        &self,
        invocation: ZomeCallInvocation,
    ) -> ConductorApiResult<ZomeCallInvocationResult> {
        self.conductor_handle.call_zome(invocation).await
    }
}

#[async_trait::async_trait]
impl InterfaceApi for RealAppInterfaceApi {
    type ApiRequest = AppRequest;
    type ApiResponse = AppResponse;
    async fn handle_request(
        &self,
        request: Result<Self::ApiRequest, SerializedBytesError>,
    ) -> InterfaceResult<Self::ApiResponse> {
        self.conductor_handle
            .check_running()
            .await
            .map_err(InterfaceError::RequestHandler)?;
        match request {
            Ok(request) => Ok(AppInterfaceApi::handle_request(self, request).await),
            Err(e) => Ok(AppResponse::Error(SerializationError::from(e).into())),
        }
    }
}
/// Responses to requests received on an App interface
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SerializedBytes)]
#[serde(rename = "snake-case", tag = "type", content = "data")]
pub enum AppResponse {
    /// There has been an error in the request
    Error(ExternalApiWireError),
    /// The response to a zome call
    ZomeCallInvocationResponse {
        /// The data that was returned by this call
        response: Box<ZomeCallInvocationResponse>,
    },
}

/// Responses to messages received on an Admin interface
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SerializedBytes)]
#[serde(rename = "snake-case", tag = "type", content = "data")]
pub enum AdminResponse {
    /// This response is unimplemented
    Unimplemented(AdminRequest),
    /// hApp [Dna]s have successfully been installed
    AppInstalled,
    /// AdminInterfaces have successfully been added
    AdminInterfacesAdded(()),
    /// A list of all installed [Dna]s
    ListDnas(Vec<DnaHash>),
    /// Keystore generated a new AgentPubKey
    GenerateAgentPubKey(AgentPubKey),
    /// Listing all the AgentPubKeys in the Keystore
    ListAgentPubKeys(Vec<AgentPubKey>),
    /// [AppInterfaceApi] successfully attached
    AppInterfaceAttached {
        /// Port of the new [AppInterfaceApi]
        port: u16,
    },
    /// An error has ocurred in this request
    Error(ExternalApiWireError),
    /// App activated successfully
    AppActivated,
    /// App deactivated successfully
    AppDeactivated,
    /// State of a cell
    JsonState(String),
}

/// The set of messages that a conductor understands how to handle over an App interface
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SerializedBytes)]
#[serde(rename = "snake-case", tag = "type", content = "data")]
pub enum AppRequest {
    /// Asks the conductor to do some crypto
    CryptoRequest {
        /// The request payload
        request: Box<CryptoRequest>,
    },
    /// Call a zome function
    ZomeCallInvocationRequest {
        /// Information about which zome call you want to make
        request: Box<ZomeCallInvocation>,
    },
}

/// The set of messages that a conductor understands how to handle over an Admin interface
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, SerializedBytes)]
#[serde(rename = "snake-case", tag = "type", content = "data")]
pub enum AdminRequest {
    /// Start a cell running
    Start(CellHandle),
    /// Stop a cell running
    Stop(CellHandle),
    /// Set up and register an Admin interface task
    AddAdminInterfaces(Vec<AdminInterfaceConfig>),
    /// Install an app from a list of Dna paths
    /// Triggers genesis to be run on all cells and
    /// Dnas to be stored
    InstallApp {
        /// App Id, [AgentPubKey] and paths to Dnas
        app_paths: AppPaths,
        /// Optional membrane proofs for Dnas
        proofs: MembraneProofs,
    },
    /// List all installed [Dna]s
    ListDnas,
    /// Generate a new AgentPubKey
    GenerateAgentPubKey,
    /// List all AgentPubKeys in Keystore
    ListAgentPubKeys,
    /// Activate an app
    ActivateApp {
        /// The id of the app to activate
        app_id: AppId,
    },
    /// Deactivate an app
    DeactivateApp {
        /// The id of the app to deactivate
        app_id: AppId,
    },
    /// Attach a [AppInterfaceApi]
    AttachAppInterface {
        /// Optional port, use None to let the
        /// OS choose a free port
        port: Option<u16>,
    },
    /// Dump the state of a cell
    DumpState(CellId),
}

#[allow(missing_docs)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename = "snake-case", tag = "type", content = "data")]
pub enum CryptoRequest {
    Sign(String),
    Decrypt(String),
    Encrypt(String),
}

#[allow(missing_docs)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename = "snake-case", tag = "type", content = "data")]
pub enum TestRequest {
    AddAgent(AddAgentArgs),
}

#[allow(dead_code, missing_docs)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AddAgentArgs {
    id: String,
    name: String,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::conductor::Conductor;
    use anyhow::Result;
    use holochain_state::test_utils::{test_conductor_env, test_wasm_env, TestEnvironment};
    use holochain_types::{
        observability,
        test_utils::{fake_agent_pubkey_1, fake_dna_file, write_fake_dna_file},
    };
    use matches::assert_matches;
    use uuid::Uuid;

    #[tokio::test(threaded_scheduler)]
    async fn install_list_dna() -> Result<()> {
        observability::test_run().ok();
        let test_env = test_conductor_env();
        let TestEnvironment {
            env: wasm_env,
            tmpdir: _tmpdir,
        } = test_wasm_env();
        let _tmpdir = test_env.tmpdir.clone();
        let handle = Conductor::builder().test(test_env, wasm_env).await?;
        let admin_api = RealAdminInterfaceApi::new(handle);
        let uuid = Uuid::new_v4();
        let dna = fake_dna_file(&uuid.to_string());
        let (dna_path, _tempdir) = write_fake_dna_file(dna.clone()).await.unwrap();
        let dna_hash = dna.dna_hash().clone();
        let agent_key = fake_agent_pubkey_1();
        let app_paths = AppPaths {
            dnas: vec![(dna_path, None)],
            app_id: "test".to_string(),
            agent_key,
        };
        let proofs = MembraneProofs::empty();
        let install_response = admin_api
            .handle_admin_request(AdminRequest::InstallApp { app_paths, proofs })
            .await;
        assert_matches!(install_response, AdminResponse::AppInstalled);
        let dna_list = admin_api.handle_admin_request(AdminRequest::ListDnas).await;
        let expects = vec![dna_hash];
        assert_matches!(dna_list, AdminResponse::ListDnas(a) if a == expects);
        Ok(())
    }

    #[tokio::test(threaded_scheduler)]
    async fn generate_and_list_pub_keys() -> Result<()> {
        let test_env = test_conductor_env();
        let TestEnvironment {
            env: wasm_env,
            tmpdir: _tmpdir,
        } = test_wasm_env();
        let _tmpdir = test_env.tmpdir.clone();
        let handle = Conductor::builder().test(test_env, wasm_env).await.unwrap();
        let admin_api = RealAdminInterfaceApi::new(handle);

        let agent_pub_key = admin_api
            .handle_admin_request(AdminRequest::GenerateAgentPubKey)
            .await;

        let agent_pub_key = match agent_pub_key {
            AdminResponse::GenerateAgentPubKey(key) => key,
            _ => panic!("bad type: {:?}", agent_pub_key),
        };

        let pub_key_list = admin_api
            .handle_admin_request(AdminRequest::ListAgentPubKeys)
            .await;

        let mut pub_key_list = match pub_key_list {
            AdminResponse::ListAgentPubKeys(list) => list,
            _ => panic!("bad type: {:?}", pub_key_list),
        };

        // includes our two pre-generated test keys
        let mut expects = vec![
            AgentPubKey::try_from("uhCAkw-zrttiYpdfAYX4fR6W8DPUdheZJ-1QsRA4cTImmzTYUcOr4").unwrap(),
            AgentPubKey::try_from("uhCAkomHzekU0-x7p62WmrusdxD2w9wcjdajC88688JGSTEo6cbEK").unwrap(),
            agent_pub_key,
        ];

        pub_key_list.sort();
        expects.sort();

        assert_eq!(expects, pub_key_list);

        Ok(())
    }

    #[tokio::test(threaded_scheduler)]
    async fn dna_read_parses() -> Result<()> {
        let uuid = Uuid::new_v4();
        let dna = fake_dna_file(&uuid.to_string());
        let (dna_path, _tmpdir) = write_fake_dna_file(dna.clone()).await?;
        let json = serde_json::json!({
            "test": "example",
            "how_many": 42,
        });
        let properties = Some(json.clone());
        let result = read_parse_dna(dna_path, properties).await?;
        let properties = Properties::new(json);
        let mut dna = dna.dna().clone();
        dna.properties = properties.try_into().unwrap();
        assert_eq!(&dna, result.dna());
        Ok(())
    }
}
