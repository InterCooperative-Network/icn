# Workshop 9: Local Deployment and Observability

## Goal
Run a local deployment and verify health and metrics endpoints.

## Steps
1. Review `deploy/README.md` and `config/README.md`
2. Start ICN with Docker Compose or native run
3. Verify health: `curl http://localhost:8080/health` (or configured port)
4. Verify metrics: `curl http://localhost:9100/metrics`

## Checkpoints
- You can explain the deployment option you used
- You can confirm health and metrics endpoints are reachable
