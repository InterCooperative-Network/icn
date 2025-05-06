(module
  ;; Import host functions
  (import "env" "log" (func $log (param i32 i32)))
  (import "env" "get_caller_did" (func $get_caller_did (result i32)))
  
  ;; Memory section
  (memory (export "memory") 1)
  
  ;; Data section for static strings
  (data (i32.const 0) "Hello from WASM module")
  (data (i32.const 32) "Test function executed")
  
  ;; Export a test function that returns an i32
  (func (export "test_function") (result i32)
    ;; Local variables must be declared at the beginning of the function
    (local $i i32)
    (local $sum i32)
    
    ;; Call log with a static message
    (call $log 
      (i32.const 32)  ;; pointer to "Test function executed"
      (i32.const 21)  ;; length of message
    )
    
    ;; Call get_caller_did to test host function
    (drop (call $get_caller_did))
    
    ;; Add some computation to test resource tracking
    (local.set $i (i32.const 0))
    (local.set $sum (i32.const 0))
    
    (block $break
      (loop $top
        ;; Increment sum
        (local.set $sum (i32.add (local.get $sum) (local.get $i)))
        
        ;; Increment i
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        
        ;; If i < 1000, continue loop
        (br_if $top (i32.lt_s (local.get $i) (i32.const 1000)))
      )
    )
    
    ;; Return success
    (i32.const 42)
  )
  
  ;; Export another test function that returns the caller's DID
  (func (export "get_caller") (result i32)
    (call $get_caller_did)
  )
) 