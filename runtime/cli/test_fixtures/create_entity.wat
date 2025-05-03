(module
  ;; Import the host_create_sub_dag function
  (import "env" "host_create_sub_dag" 
    (func $host_create_sub_dag 
      (param $parent_did_ptr i32) (param $parent_did_len i32)
      (param $genesis_payload_ptr i32) (param $genesis_payload_len i32)
      (param $entity_type_ptr i32) (param $entity_type_len i32)
      (param $did_out_ptr i32) (param $did_out_max_len i32)
      (result i32)
    )
  )
  
  ;; Import memory
  (import "env" "memory" (memory 1))
  
  ;; Logging function for debugging
  (import "env" "host_log" (func $host_log (param i32 i32) (result i32)))
  
  ;; Export the main function
  (export "main" (func $main))
  
  ;; Define constants for string locations in memory
  (global $parent_did_offset i32 (i32.const 0))
  (global $genesis_payload_offset i32 (i32.const 64))
  (global $entity_type_offset i32 (i32.const 512))
  (global $did_out_offset i32 (i32.const 576))
  
  ;; Load sample data into memory
  (data (i32.const 0) "did:icn:federation") ;; Parent DID
  
  ;; Genesis payload (simple JSON as CBOR bytes)
  (data (i32.const 64) "\xA3\x64\x6E\x61\x6D\x65\x70\x54\x65\x73\x74\x20\x43\x6F\x6F\x70\x65\x72\x61\x74\x69\x76\x65\x6B\x64\x65\x73\x63\x72\x69\x70\x74\x69\x6F\x6E\x78\x1A\x41\x20\x63\x6F\x6F\x70\x65\x72\x61\x74\x69\x76\x65\x20\x63\x72\x65\x61\x74\x65\x64\x20\x66\x6F\x72\x20\x74\x65\x73\x74\x69\x6E\x67\x6A\x63\x72\x65\x61\x74\x65\x64\x5F\x61\x74\x1A\x64\x7A\x5A\xB0")
  
  ;; Entity type
  (data (i32.const 512) "Cooperative")
  
  ;; Main function
  (func $main (result i32)
    ;; Define string lengths
    (local $parent_did_len i32)
    (local $genesis_payload_len i32)
    (local $entity_type_len i32)
    (local $result i32)
    
    ;; Set string lengths
    (local.set $parent_did_len (i32.const 17)) ;; "did:icn:federation"
    (local.set $genesis_payload_len (i32.const 100)) ;; Approximate CBOR length
    (local.set $entity_type_len (i32.const 11)) ;; "Cooperative"
    
    ;; Log that we're starting
    (drop (call $host_log 
      (i32.const 512) ;; Reuse the entity_type buffer as log message
      (local.get $entity_type_len)
    ))
    
    ;; Call host_create_sub_dag
    (local.set $result (call $host_create_sub_dag
      (global.get $parent_did_offset)
      (local.get $parent_did_len)
      (global.get $genesis_payload_offset)
      (local.get $genesis_payload_len)
      (global.get $entity_type_offset)
      (local.get $entity_type_len)
      (global.get $did_out_offset)
      (i32.const 100) ;; Max DID output length
    ))
    
    ;; Check if creation was successful
    (if (i32.lt_s (local.get $result) (i32.const 0))
      (then
        ;; Return error code
        (return (local.get $result))
      )
    )
    
    ;; Return success (length of the returned DID)
    (local.get $result)
  )
) 