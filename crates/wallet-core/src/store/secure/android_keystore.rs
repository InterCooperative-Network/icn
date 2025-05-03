use async_trait::async_trait;
use jni::JNIEnv;
use jni::objects::{JObject, JString, JValue};
use jni::sys::{jbyteArray, jobject};
use std::sync::{Arc, Mutex};
use tokio::task;
use crate::error::{WalletResult, WalletError};
use super::super::secure::SecureStorageProvider;

/// Android Keystore provider implementation for secure storage
/// Uses the Android Keystore System to securely store and retrieve keys
#[derive(Clone)]
pub struct AndroidKeystoreProvider {
    /// JNI interface cache (lazily initialized)
    jni_cache: Arc<Mutex<Option<JniCache>>>,
    /// Keystore keys prefix to isolate from other applications
    prefix: String,
}

/// Cache for JNI objects to avoid repeated lookups
struct JniCache {
    /// The class reference for the KeystoreHelper
    keystore_helper_class: jni::objects::GlobalRef,
    /// Method ID for storing data
    store_method: jni::objects::JMethodID,
    /// Method ID for retrieving data
    retrieve_method: jni::objects::JMethodID,
    /// Method ID for deleting data
    delete_method: jni::objects::JMethodID,
    /// Method ID for checking if a key exists
    exists_method: jni::objects::JMethodID,
    /// Method ID for listing all keys
    list_keys_method: jni::objects::JMethodID,
}

impl AndroidKeystoreProvider {
    /// Create a new instance of the Android Keystore provider
    pub fn new() -> Self {
        Self {
            jni_cache: Arc::new(Mutex::new(None)),
            prefix: "icn.wallet.".to_string(),
        }
    }
    
    /// Get or initialize the JNI cache
    fn get_or_init_jni_cache(&self, env: &JNIEnv) -> jni::errors::Result<&JniCache> {
        let mut cache = self.jni_cache.lock().unwrap();
        if cache.is_none() {
            // Find the KeystoreHelper class
            let keystore_helper_class = env.find_class("com/icn/wallet/security/KeystoreHelper")?;
            let keystore_helper_class = env.new_global_ref(keystore_helper_class)?;
            
            // Get method IDs
            let store_method = env.get_method_id(
                &keystore_helper_class,
                "storeData",
                "(Ljava/lang/String;[B)Z"
            )?;
            
            let retrieve_method = env.get_method_id(
                &keystore_helper_class,
                "retrieveData",
                "(Ljava/lang/String;)[B"
            )?;
            
            let delete_method = env.get_method_id(
                &keystore_helper_class,
                "deleteData",
                "(Ljava/lang/String;)Z"
            )?;
            
            let exists_method = env.get_method_id(
                &keystore_helper_class,
                "exists",
                "(Ljava/lang/String;)Z"
            )?;
            
            let list_keys_method = env.get_method_id(
                &keystore_helper_class,
                "listKeys",
                "()[Ljava/lang/String;"
            )?;
            
            *cache = Some(JniCache {
                keystore_helper_class,
                store_method,
                retrieve_method,
                delete_method,
                exists_method,
                list_keys_method,
            });
        }
        
        // Unwrap is safe because we just set the value if it was None
        Ok(cache.as_ref().unwrap())
    }
    
    /// Get the keystore helper instance
    fn get_keystore_helper(&self, env: &JNIEnv) -> jni::errors::Result<JObject> {
        let cache = self.get_or_init_jni_cache(env)?;
        
        // Create a new instance of KeystoreHelper
        let obj = env.new_object(
            &cache.keystore_helper_class,
            "()V",
            &[]
        )?;
        
        Ok(obj)
    }
    
    /// Get a prefixed key for storage
    fn get_prefixed_key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }
}

#[async_trait]
impl SecureStorageProvider for AndroidKeystoreProvider {
    async fn store(&self, key: &str, data: &[u8]) -> WalletResult<()> {
        let provider = self.clone();
        let key = self.get_prefixed_key(key);
        let data = data.to_vec();
        
        // Run the JNI calls on a separate thread to avoid blocking the async runtime
        task::spawn_blocking(move || -> WalletResult<()> {
            // Attach the current thread to the JVM
            let vm = jni::JavaVM::get_java_vm_unwrap();
            let env = vm.attach_current_thread_as_daemon()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the keystore helper
            let keystore_helper = provider.get_keystore_helper(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Convert key to JString
            let j_key = env.new_string(&key)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Convert data to jbyteArray
            let j_data = env.byte_array_from_slice(&data)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            let cache = provider.get_or_init_jni_cache(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Call the store method
            let result = env.call_method_unchecked(
                keystore_helper,
                cache.store_method,
                jni::signature::ReturnType::Primitive(jni::signature::Primitive::Boolean),
                &[
                    JValue::Object(j_key.into()),
                    JValue::Object(j_data.into()),
                ]
            ).map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Check the result
            let success = result.z()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            if !success {
                return Err(WalletError::StorageError(format!("Failed to store key {}", key)));
            }
            
            Ok(())
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn retrieve(&self, key: &str) -> WalletResult<Vec<u8>> {
        let provider = self.clone();
        let key = self.get_prefixed_key(key);
        
        // Run the JNI calls on a separate thread to avoid blocking the async runtime
        task::spawn_blocking(move || -> WalletResult<Vec<u8>> {
            // Attach the current thread to the JVM
            let vm = jni::JavaVM::get_java_vm_unwrap();
            let env = vm.attach_current_thread_as_daemon()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the keystore helper
            let keystore_helper = provider.get_keystore_helper(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Convert key to JString
            let j_key = env.new_string(&key)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            let cache = provider.get_or_init_jni_cache(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Call the retrieve method
            let result = env.call_method_unchecked(
                keystore_helper,
                cache.retrieve_method,
                jni::signature::ReturnType::Object,
                &[JValue::Object(j_key.into())]
            ).map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the byte array
            let j_data = result.l()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Check if data is null (key not found)
            if j_data.is_null() {
                return Err(WalletError::NotFound(format!("Key not found: {}", key)));
            }
            
            // Convert jbyteArray to Vec<u8>
            let data_array = env.convert_byte_array(j_data.into_raw() as jbyteArray)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            Ok(data_array)
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn delete(&self, key: &str) -> WalletResult<()> {
        let provider = self.clone();
        let key = self.get_prefixed_key(key);
        
        // Run the JNI calls on a separate thread to avoid blocking the async runtime
        task::spawn_blocking(move || -> WalletResult<()> {
            // Attach the current thread to the JVM
            let vm = jni::JavaVM::get_java_vm_unwrap();
            let env = vm.attach_current_thread_as_daemon()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the keystore helper
            let keystore_helper = provider.get_keystore_helper(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Convert key to JString
            let j_key = env.new_string(&key)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            let cache = provider.get_or_init_jni_cache(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Call the delete method
            let result = env.call_method_unchecked(
                keystore_helper,
                cache.delete_method,
                jni::signature::ReturnType::Primitive(jni::signature::Primitive::Boolean),
                &[JValue::Object(j_key.into())]
            ).map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Check the result
            let success = result.z()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            if !success {
                return Err(WalletError::NotFound(format!("Key not found or could not be deleted: {}", key)));
            }
            
            Ok(())
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn exists(&self, key: &str) -> WalletResult<bool> {
        let provider = self.clone();
        let key = self.get_prefixed_key(key);
        
        // Run the JNI calls on a separate thread to avoid blocking the async runtime
        task::spawn_blocking(move || -> WalletResult<bool> {
            // Attach the current thread to the JVM
            let vm = jni::JavaVM::get_java_vm_unwrap();
            let env = vm.attach_current_thread_as_daemon()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the keystore helper
            let keystore_helper = provider.get_keystore_helper(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Convert key to JString
            let j_key = env.new_string(&key)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            let cache = provider.get_or_init_jni_cache(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Call the exists method
            let result = env.call_method_unchecked(
                keystore_helper,
                cache.exists_method,
                jni::signature::ReturnType::Primitive(jni::signature::Primitive::Boolean),
                &[JValue::Object(j_key.into())]
            ).map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the boolean result
            let exists = result.z()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            Ok(exists)
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
    
    async fn list_keys(&self) -> WalletResult<Vec<String>> {
        let provider = self.clone();
        let prefix = self.prefix.clone();
        
        // Run the JNI calls on a separate thread to avoid blocking the async runtime
        task::spawn_blocking(move || -> WalletResult<Vec<String>> {
            // Attach the current thread to the JVM
            let vm = jni::JavaVM::get_java_vm_unwrap();
            let env = vm.attach_current_thread_as_daemon()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the keystore helper
            let keystore_helper = provider.get_keystore_helper(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            let cache = provider.get_or_init_jni_cache(&env)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Call the list_keys method
            let result = env.call_method_unchecked(
                keystore_helper,
                cache.list_keys_method,
                jni::signature::ReturnType::Object,
                &[]
            ).map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Get the string array
            let j_keys = result.l()
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            // Check if keys is null
            if j_keys.is_null() {
                return Ok(Vec::new());
            }
            
            // Get array length
            let length = env.get_array_length(j_keys.into_raw() as jobject)
                .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
            
            let mut keys = Vec::with_capacity(length as usize);
            
            // Process each key
            for i in 0..length {
                // Get the string at index i
                let j_key = env.get_object_array_element(j_keys.into_raw() as jobject, i)
                    .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?;
                
                // Convert JString to Rust String
                let key: String = env.get_string(JString::from(j_key))
                    .map_err(|e| WalletError::PlatformError(format!("JNI error: {}", e)))?
                    .into();
                
                // Filter only keys with our prefix, and remove the prefix for the result
                if key.starts_with(&prefix) {
                    keys.push(key.replacen(&prefix, "", 1));
                }
            }
            
            Ok(keys)
        }).await.map_err(|e| WalletError::RuntimeError(format!("Task error: {}", e)))?
    }
} 