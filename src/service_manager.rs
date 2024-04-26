use std::ffi::{OsStr, OsString};
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::System::Services;

use crate::sc_handle::ScHandle;
use crate::service::{Service, ServiceAccess, ServiceInfo};
use crate::{Error, Result};

bitflags::bitflags! {
    /// Flags describing access permissions for [`ServiceManager`].
    #[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Copy, Clone, Hash)]
    pub struct ServiceManagerAccess: u32 {
        /// Can connect to service control manager.
        const CONNECT = Services::SC_MANAGER_CONNECT;

        /// Can create services.
        const CREATE_SERVICE = Services::SC_MANAGER_CREATE_SERVICE;

        /// Can enumerate services or receive notifications.
        const ENUMERATE_SERVICE = Services::SC_MANAGER_ENUMERATE_SERVICE;

        /// Includes all possible access rights.
        const ALL_ACCESS = Services::SC_MANAGER_ALL_ACCESS;
    }
}

/// Service manager.
pub struct ServiceManager {
    manager_handle: ScHandle,
}

impl ServiceManager {
    /// Private initializer.
    ///
    /// # Arguments
    ///
    /// * `machine` - The name of machine. Pass `None` to connect to local machine.
    /// * `database` - The name of database to connect to. Pass `None` to connect to active
    ///   database.
    fn new(
        machine: Option<impl AsRef<OsStr>>,
        database: Option<impl AsRef<OsStr>>,
        request_access: ServiceManagerAccess,
    ) -> Result<Self> {
        let machine_name = machine.map(|s| HSTRING::from(s.as_ref()));
        let database_name = database.map(|s| HSTRING::from(s.as_ref()));

        let manager_handle = unsafe {
            Services::OpenSCManagerW(
                machine_name.map_or(PCWSTR::null(), |s| PCWSTR::from_raw(s.as_ptr())),
                database_name.map_or(PCWSTR::null(), |s| PCWSTR::from_raw(s.as_ptr())),
                request_access.bits(),
            )
            .map(ScHandle::new)
            .map_err(Error::Winapi)?
        };

        Ok(Self { manager_handle })
    }

    /// Connect to local services database.
    ///
    /// # Arguments
    ///
    /// * `database` - The name of database to connect to. Pass `None` to connect to active
    ///   database.
    /// * `request_access` - Desired access permissions.
    pub fn local_computer(
        database: Option<impl AsRef<OsStr>>,
        request_access: ServiceManagerAccess,
    ) -> Result<Self> {
        ServiceManager::new(None::<&OsStr>, database, request_access)
    }

    /// Connect to remote services database.
    ///
    /// # Arguments
    ///
    /// * `machine` - The name of remote machine.
    /// * `database` - The name of database to connect to. Pass `None` to connect to active
    ///   database.
    /// * `request_access` - desired access permissions.
    pub fn remote_computer(
        machine: impl AsRef<OsStr>,
        database: Option<impl AsRef<OsStr>>,
        request_access: ServiceManagerAccess,
    ) -> Result<Self> {
        ServiceManager::new(Some(machine), database, request_access)
    }

    /// Create a service.
    ///
    /// # Arguments
    ///
    /// * `service_info` - The service information that will be saved to the system services
    ///   registry.
    /// * `service_access` - Desired access permissions for the returned [`Service`] instance.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use std::ffi::OsString;
    /// use std::path::PathBuf;
    /// use windows_service::service::{
    ///     ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
    /// };
    /// use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    ///
    /// fn main() -> windows_service::Result<()> {
    ///     let manager =
    ///         ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)?;
    ///
    ///     let my_service_info = ServiceInfo {
    ///         name: OsString::from("my_service"),
    ///         display_name: OsString::from("My service"),
    ///         service_type: ServiceType::OWN_PROCESS,
    ///         start_type: ServiceStartType::OnDemand,
    ///         error_control: ServiceErrorControl::Normal,
    ///         executable_path: PathBuf::from(r"C:\path\to\my\service.exe"),
    ///         launch_arguments: vec![],
    ///         dependencies: vec![],
    ///         account_name: None, // run as System
    ///         account_password: None,
    ///     };
    ///
    ///     let my_service = manager.create_service(&my_service_info, ServiceAccess::QUERY_STATUS)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn create_service(
        &self,
        service_info: &ServiceInfo,
        service_access: ServiceAccess,
    ) -> Result<Service> {
        let account_name = service_info.account_name.as_ref().map(|s| HSTRING::from(s));
        let account_password = service_info
            .account_password
            .as_ref()
            .map(|s| HSTRING::from(s));

        let dependencies = service_info
            .raw_dependencies()?
            .map(|s| HSTRING::from(s.to_os_string()));

        let service_handle = unsafe {
            Services::CreateServiceW(
                self.manager_handle.raw_handle(),
                &HSTRING::from(&service_info.name),
                &HSTRING::from(&service_info.display_name),
                service_access.bits(),
                Services::ENUM_SERVICE_TYPE(service_info.service_type.bits()),
                Services::SERVICE_START_TYPE(service_info.start_type.to_raw()),
                Services::SERVICE_ERROR(service_info.error_control.to_raw()),
                &HSTRING::from(service_info.raw_launch_command()?.to_os_string()),
                PCWSTR::null(), // load ordering group
                None,           // tag id within the load ordering group
                dependencies.map_or(PCWSTR::null(), |s| PCWSTR::from_raw(s.as_ptr())),
                account_name.map_or(PCWSTR::null(), |s| PCWSTR::from_raw(s.as_ptr())),
                account_password.map_or(PCWSTR::null(), |s| PCWSTR::from_raw(s.as_ptr())),
            )
            .map(ScHandle::new)
            .map_err(Error::Winapi)?
        };

        Ok(Service::new(service_handle))
    }

    /// Open an existing service.
    ///
    /// # Arguments
    ///
    /// * `name` - The service name.
    /// * `request_access` - Desired permissions for the returned [`Service`] instance.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use windows_service::service::ServiceAccess;
    /// use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    ///
    /// # fn main() -> windows_service::Result<()> {
    /// let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    /// let my_service = manager.open_service("my_service", ServiceAccess::QUERY_STATUS)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn open_service(
        &self,
        name: impl AsRef<OsStr>,
        request_access: ServiceAccess,
    ) -> Result<Service> {
        let service_handle = unsafe {
            Services::OpenServiceW(
                self.manager_handle.raw_handle(),
                &HSTRING::from(name.as_ref()),
                request_access.bits(),
            )
            .map_err(Error::Winapi)?
        };

        Ok(Service::new(ScHandle::new(service_handle)))
    }

    /// Return the service name given a service display name.
    ///
    /// # Arguments
    ///
    /// * `name` - A service display name.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    ///
    /// # fn main() -> windows_service::Result<()> {
    /// let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    /// let my_service_name = manager.service_name_from_display_name("My Service Display Name")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn service_name_from_display_name(
        &self,
        display_name: impl AsRef<OsStr>,
    ) -> Result<OsString> {
        // As per docs, the maximum size of data buffer used by GetServiceKeyNameW is 4k bytes,
        // which is 2k wchars
        let mut buffer = [0u16; 2 * 1024];
        let mut buffer_len = u32::try_from(buffer.len()).expect("size must fit in u32");

        let str_buffer = PWSTR::from_raw(buffer.as_mut_ptr());

        unsafe {
            Services::GetServiceKeyNameW(
                self.manager_handle.raw_handle(),
                &HSTRING::from(display_name.as_ref()),
                str_buffer,
                &mut buffer_len,
            )
            .map_err(Error::Winapi)?;
        }

        unsafe { str_buffer.to_hstring() }
            .map(|s| s.to_os_string())
            .map_err(Error::Winapi)
    }
}
