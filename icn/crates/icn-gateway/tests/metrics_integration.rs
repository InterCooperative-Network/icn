use actix_web::App;
use actix_web_prom::PrometheusMetricsBuilder;
use icn_gateway::api;
use prometheus::Registry;

#[actix_web::test]
async fn test_metrics_endpoint() {
    // Create our own registry so we can inspect it
    let registry = Registry::new();
    
    // Initialize Prometheus metrics with the custom registry
    let prometheus = PrometheusMetricsBuilder::new("api")
        .registry(registry.clone())
        .endpoint("/metrics")
        .build()
        .unwrap();

    let app = actix_web::test::init_service(
        App::new()
            .wrap(prometheus)
            .service(api::health::health)
    )
    .await;

    // Make a request to a normal endpoint to generate some metrics
    let req = actix_web::test::TestRequest::get()
        .uri("/health")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // Check the registry for metrics
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(metric_families.as_slice(), &mut buffer).unwrap();
    let metrics = String::from_utf8(buffer).unwrap();
    
    println!("Metrics output:\n{}", metrics);
    
    // Verify metrics exist
    assert!(!metrics.is_empty(), "Metrics should exist after request");
    assert!(metrics.contains("api_http_requests_total"), 
        "Should contain total requests metric");
    assert!(metrics.contains("api_http_requests_duration_seconds"), 
        "Should contain duration metric");
}
