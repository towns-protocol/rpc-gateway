[ ] Create configuration: The initial readiness probe should validate all upstreams are healthy and they match the chain id.
[ ] Config: watch the config.yml file and automatically update the config in the running pod.
[ ] Remove struct name artifacts from json logs
[ ] Why are debug logs not showing?
[ ] What's the current restart policy in k8s?
[ ] Add graceful shutdowns
[ ] Liveness probe should run liveness checks, and readiness probe should run readiness checks: remove all references to liveness from readiness and vice versa.
[ ] Increase internal health check default period to 5 minutes
[ ] Internal health check should be configurable:

- [ ] Period
- [ ] Timeout
- [ ] Allow externally inbound health checks to actually execute the health checks
- [ ] Allow the server to refuse starting unless all upstreams are healthy (configurable)

[ ] Health checks should run with a task manager
[ ] Add graceful shutdowns
[ ] Update all duration configs to use the default format: i.e "10s" or "100ms" etc
[ ] Stop depending on the RUST_LOG environment variable. But do allow it to take effect if set.
