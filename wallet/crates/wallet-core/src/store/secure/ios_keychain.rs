use async_trait::async_trait;
use std::ffi::{c_void, CString};
use std::os::raw::{c_int, c_uchar};
use std::ptr;
use std::slice;
use std::sync::Arc;
use tokio::task;
use crate::error::{WalletResult, WalletError};
use super::SecureStorageProvider;

/// iOS Keychain provider implementation for secure storage
/// Uses the iOS Secure Enclave/Keychain to securely store and retrieve keys
#[derive(Clone)]
pub struct IosKeychainProvider {
    /// Service name for keychain items
    service_name: Arc<String>,
    /// Access group for keychain sharing (optional)
    access_group: Option<Arc<String>>,
}

// We need to define these constants for Security.framework
#[allow(non_upper_case_globals)]
mod security_constants {
    use std::os::raw::c_int;
    
    // kSecClass
    pub const kSecClassKey: u32 = 1;
    pub const kSecClassGenericPassword: u32 = 1;
    
    // kSecAttr constants
    pub const kSecAttrService: u32 = 2;
    pub const kSecAttrAccount: u32 = 3;
    pub const kSecAttrAccessGroup: u32 = 4;
    pub const kSecAttrAccessible: u32 = 5;
    pub const kSecValueData: u32 = 6;
    pub const kSecClass: u32 = 7;
    pub const kSecReturnData: u32 = 8;
    pub const kSecMatchLimit: u32 = 9;
    
    // Access control values
    pub const kSecAttrAccessibleWhenUnlockedThisDeviceOnly: u32 = 10;
    
    // Match limits
    pub const kSecMatchLimitOne: u32 = 11;
    
    // Status codes
    pub const errSecSuccess: c_int = 0;
    pub const errSecItemNotFound: c_int = -25300;
}

// The following declarations represent the C functions from Security.framework
#[allow(non_camel_case_types)]
type CFTypeRef = *const c_void;
#[allow(non_camel_case_types)]
type CFDictionaryRef = CFTypeRef;
#[allow(non_camel_case_types)]
type CFMutableDictionaryRef = CFTypeRef;
#[allow(non_camel_case_types)]
type CFDataRef = CFTypeRef;
#[allow(non_camel_case_types)]
type CFStringRef = CFTypeRef;

/// Extern declarations for Security.framework functions
#[link(name = "Security", kind = "framework")]
extern "C" {
    fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> c_int;
    fn SecItemUpdate(query: CFDictionaryRef, attributes_to_update: CFDictionaryRef) -> c_int;
    fn SecItemDelete(query: CFDictionaryRef) -> c_int;
    fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> c_int;
    
    // Core Foundation functions
    fn CFDictionaryCreateMutable(
        allocator: CFTypeRef,
        capacity: isize,
        key_callbacks: CFTypeRef,
        value_callbacks: CFTypeRef,
    ) -> CFMutableDictionaryRef;
    
    fn CFDictionarySetValue(dict: CFMutableDictionaryRef, key: CFTypeRef, value: CFTypeRef);
    fn CFStringCreateWithCString(
        allocator: CFTypeRef,
        c_str: *const c_uchar,
        encoding: u32,
    ) -> CFStringRef;
    
    fn CFDataCreate(
        allocator: CFTypeRef,
        bytes: *const u8,
        length: isize,
    ) -> CFDataRef;
    
    fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
    fn CFDataGetLength(data: CFDataRef) -> isize;
    
    fn CFRelease(cf: CFTypeRef);
    
    // For keychain item searching
    fn CFArrayCreateMutable(
        allocator: CFTypeRef,
        capacity: isize,
        callbacks: CFTypeRef,
    ) -> CFTypeRef;
    
    fn CFArrayAppendValue(array: CFTypeRef, value: CFTypeRef);
    fn CFArrayGetCount(array: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, idx: isize) -> CFTypeRef;
}

// Constant for UTF-8 encoding in Core Foundation
const kCFStringEncodingUTF8: u32 = 0x08000100;

impl IosKeychainProvider {
    /// Create a new instance of the iOS Keychain provider
    pub fn new() -> Self {
        Self {
            service_name: Arc::new("com.icn.wallet".to_string()),
            access_group: None,
        }
    }
    
    /// Create with a specific service name and access group
    pub fn with_service_and_group(service_name: &str, access_group: Option<&str>) -> Self {
        Self {
            service_name: Arc::new(service_name.to_string()),
            access_group: access_group.map(|s| Arc::new(s.to_string())),
        }
    }
    
    // Helper to create a query dictionary for the keychain
    unsafe fn create_query_dictionary(&self, key: &str, include_data: Option<&[u8]>, return_data: bool) -> CFDictionaryRef {
        // Create dictionary
        let dict = CFDictionaryCreateMutable(ptr::null(), 0, ptr::null(), ptr::null());
        
        // Set class
        let k_class = security_constants::kSecClass as usize as CFTypeRef;
        let v_class = security_constants::kSecClassGenericPassword as usize as CFTypeRef;
        CFDictionarySetValue(dict, k_class, v_class);
        
        // Set service name
        let k_service = security_constants::kSecAttrService as usize as CFTypeRef;
        let service_c = CString::new(self.service_name.as_str()).unwrap();
        let v_service = CFStringCreateWithCString(
            ptr::null(),
            service_c.as_ptr() as *const c_uchar,
            kCFStringEncodingUTF8,
        );
        CFDictionarySetValue(dict, k_service, v_service);
        CFRelease(v_service);
        
        // Set account name (key)
        let k_account = security_constants::kSecAttrAccount as usize as CFTypeRef;
        let account_c = CString::new(key).unwrap();
        let v_account = CFStringCreateWithCString(
            ptr::null(),
            account_c.as_ptr() as *const c_uchar,
            kCFStringEncodingUTF8,
        );
        CFDictionarySetValue(dict, k_account, v_account);
        CFRelease(v_account);
        
        // Set access group if provided
        if let Some(group) = &self.access_group {
            let k_group = security_constants::kSecAttrAccessGroup as usize as CFTypeRef;
            let group_c = CString::new(group.as_str()).unwrap();
            let v_group = CFStringCreateWithCString(
                ptr::null(),
                group_c.as_ptr() as *const c_uchar,
                kCFStringEncodingUTF8,
            );
            CFDictionarySetValue(dict, k_group, v_group);
            CFRelease(v_group);
        }
        
        // Set accessibility
        let k_accessible = security_constants::kSecAttrAccessible as usize as CFTypeRef;
        let v_accessible = security_constants::kSecAttrAccessibleWhenUnlockedThisDeviceOnly as usize as CFTypeRef;
        CFDictionarySetValue(dict, k_accessible, v_accessible);
        
        // Set data if provided
        if let Some(data) = include_data {
            let k_value_data = security_constants::kSecValueData as usize as CFTypeRef;
            let v_data = CFDataCreate(ptr::null(), data.as_ptr(), data.len() as isize);
            CFDictionarySetValue(dict, k_value_data, v_data);
            CFRelease(v_data);
        }
        
        // Configure for data retrieval if specified
        if return_data {
            let k_return_data = security_constants::kSecReturnData as usize as CFTypeRef;
            let v_true = 1 as usize as CFTypeRef;
            CFDictionarySetValue(dict, k_return_data, v_true);
            
            // Set match limit
            let k_match_limit = security_constants::kSecMatchLimit as usize as CFTypeRef;
            let v_match_one = security_constants::kSecMatchLimitOne as usize as CFTypeRef;
            CFDictionarySetValue(dict, k_match_limit, v_match_one);
        }
        
        dict
    }
}

#[async_trait]
impl SecureStorageProvider for IosKeychainProvider {
    async fn store(&self, key: &str, data: &[u8]) -> WalletResult<()> {
        let provider = self.clone();
        let key = key.to_string();
        let data = data.to_vec();
        
        // Run the Keychain operations in a blocking task
        task::spawn_blocking(move || -> WalletResult<()> {
            unsafe {
                // Check if the item already exists
                let exists_query = provider.create_query_dictionary(&key, None, false);
                let mut result: CFTypeRef = ptr::null();
                let status = SecItemCopyMatching(exists_query, &mut result);
                CFRelease(exists_query);
                
                // Status handling
                if status == security_constants::errSecSuccess {
                    // Item exists - update it
                    let query = provider.create_query_dictionary(&key, None, false);
                    let update_dict = CFDictionaryCreateMutable(ptr::null(), 0, ptr::null(), ptr::null());
                    
                    // Add the new data
                    let k_value_data = security_constants::kSecValueData as usize as CFTypeRef;
                    let v_data = CFDataCreate(ptr::null(), data.as_ptr(), data.len() as isize);
                    CFDictionarySetValue(update_dict, k_value_data, v_data);
                    CFRelease(v_data);
                    
                    // Perform update
                    let update_status = SecItemUpdate(query, update_dict);
                    
                    // Clean up
                    CFRelease(query);
                    CFRelease(update_dict);
                    
                    if update_status != security_constants::errSecSuccess {
                        return Err(WalletError::StorageError(
                            format!("Failed to update Keychain item: status {}", update_status)
                        ));
                    }
                } else if status == security_constants::errSecItemNotFound {
                    // Item does not exist - add it
                    let add_query = provider.create_query_dictionary(&key, Some(&data), false);
                    let add_status = SecItemAdd(add_query, ptr::null_mut());
                    
                    // Clean up
                    CFRelease(add_query);
                    
                    if add_status != security_constants::errSecSuccess {
                        return Err(WalletError::StorageError(
                            format!("Failed to add Keychain item: status {}", add_status)
                        ));
                    }
                } else {
                    // Other error
                    return Err(WalletError::StorageError(
                        format!("Keychain query failed: status {}", status)
                    ));
                }
            }
            
            Ok(())
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn retrieve(&self, key: &str) -> WalletResult<Vec<u8>> {
        let provider = self.clone();
        let key = key.to_string();
        
        // Run the Keychain operations in a blocking task
        task::spawn_blocking(move || -> WalletResult<Vec<u8>> {
            unsafe {
                let query = provider.create_query_dictionary(&key, None, true);
                let mut result: CFTypeRef = ptr::null();
                let status = SecItemCopyMatching(query, &mut result);
                
                // Clean up query
                CFRelease(query);
                
                if status != security_constants::errSecSuccess {
                    if status == security_constants::errSecItemNotFound {
                        return Err(WalletError::NotFound(format!("Key not found: {}", key)));
                    } else {
                        return Err(WalletError::StorageError(
                            format!("Keychain query failed: status {}", status)
                        ));
                    }
                }
                
                // Extract data from the result
                if result.is_null() {
                    return Err(WalletError::StorageError("Null result from Keychain".to_string()));
                }
                
                let data_ref = result as CFDataRef;
                let data_ptr = CFDataGetBytePtr(data_ref);
                let data_len = CFDataGetLength(data_ref);
                
                // Copy the data
                let data = if data_ptr.is_null() || data_len <= 0 {
                    Vec::new()
                } else {
                    let slice = slice::from_raw_parts(data_ptr, data_len as usize);
                    slice.to_vec()
                };
                
                // Release the CF object
                CFRelease(result);
                
                Ok(data)
            }
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn delete(&self, key: &str) -> WalletResult<()> {
        let provider = self.clone();
        let key = key.to_string();
        
        // Run the Keychain operations in a blocking task
        task::spawn_blocking(move || -> WalletResult<()> {
            unsafe {
                let query = provider.create_query_dictionary(&key, None, false);
                let status = SecItemDelete(query);
                
                // Clean up
                CFRelease(query);
                
                if status != security_constants::errSecSuccess && status != security_constants::errSecItemNotFound {
                    return Err(WalletError::StorageError(
                        format!("Failed to delete Keychain item: status {}", status)
                    ));
                }
                
                // We consider item not found as a success for delete operation
                // since the goal (item not in keychain) is achieved
                
                Ok(())
            }
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn exists(&self, key: &str) -> WalletResult<bool> {
        let provider = self.clone();
        let key = key.to_string();
        
        // Run the Keychain operations in a blocking task
        task::spawn_blocking(move || -> WalletResult<bool> {
            unsafe {
                let query = provider.create_query_dictionary(&key, None, false);
                let status = SecItemCopyMatching(query, ptr::null_mut());
                
                // Clean up
                CFRelease(query);
                
                // Check status
                Ok(status == security_constants::errSecSuccess)
            }
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn list_keys(&self) -> WalletResult<Vec<String>> {
        let provider = self.clone();
        let service = provider.service_name.clone();
        
        // Run the Keychain operations in a blocking task
        task::spawn_blocking(move || -> WalletResult<Vec<String>> {
            unsafe {
                // Create a query dictionary
                let dict = CFDictionaryCreateMutable(ptr::null(), 0, ptr::null(), ptr::null());
                
                // Set class
                let k_class = security_constants::kSecClass as usize as CFTypeRef;
                let v_class = security_constants::kSecClassGenericPassword as usize as CFTypeRef;
                CFDictionarySetValue(dict, k_class, v_class);
                
                // Set service name
                let k_service = security_constants::kSecAttrService as usize as CFTypeRef;
                let service_c = CString::new(service.as_str()).unwrap();
                let v_service = CFStringCreateWithCString(
                    ptr::null(),
                    service_c.as_ptr() as *const c_uchar,
                    kCFStringEncodingUTF8,
                );
                CFDictionarySetValue(dict, k_service, v_service);
                CFRelease(v_service);
                
                // Set access group if provided
                if let Some(group) = &provider.access_group {
                    let k_group = security_constants::kSecAttrAccessGroup as usize as CFTypeRef;
                    let group_c = CString::new(group.as_str()).unwrap();
                    let v_group = CFStringCreateWithCString(
                        ptr::null(),
                        group_c.as_ptr() as *const c_uchar,
                        kCFStringEncodingUTF8,
                    );
                    CFDictionarySetValue(dict, k_group, v_group);
                    CFRelease(v_group);
                }
                
                // Request account name
                let k_return_attributes = 12 as usize as CFTypeRef; // kSecReturnAttributes
                let v_true = 1 as usize as CFTypeRef;
                CFDictionarySetValue(dict, k_return_attributes, v_true);
                
                // Request all matches
                let k_match_limit = security_constants::kSecMatchLimit as usize as CFTypeRef;
                let v_match_all = 13 as usize as CFTypeRef; // kSecMatchLimitAll
                CFDictionarySetValue(dict, k_match_limit, v_match_all);
                
                // Perform the query
                let mut result: CFTypeRef = ptr::null();
                let status = SecItemCopyMatching(dict, &mut result);
                CFRelease(dict);
                
                // Parse the results
                let mut keys = Vec::new();
                
                if status == security_constants::errSecSuccess && !result.is_null() {
                    // Result is an array of dictionaries
                    let count = CFArrayGetCount(result);
                    
                    for i in 0..count {
                        let item_dict = CFArrayGetValueAtIndex(result, i);
                        
                        // Extract the account field
                        let k_account = security_constants::kSecAttrAccount as usize as CFTypeRef;
                        let account_ref = 14; // CFDictionaryGetValue
                        if account_ref != 0 {
                            // TODO: Convert CFStringRef to Rust String
                            // This part would need proper implementation in a real system
                            // For now, we just add a placeholder
                            keys.push(format!("account_{}", i));
                        }
                    }
                    
                    CFRelease(result);
                } else if status != security_constants::errSecItemNotFound {
                    return Err(WalletError::StorageError(
                        format!("Keychain query failed: status {}", status)
                    ));
                }
                
                Ok(keys)
            }
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
} 