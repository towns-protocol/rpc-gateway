[ ] Config: watch the config.yml file and automatically update the config in the running pod.
[ ] Remove struct name artifacts from json logs
[ ] What's the current restart policy in k8s?
[ ] Add graceful shutdowns
[ ] Liveness probe should run liveness checks, and readiness probe should run readiness checks: remove all references to liveness from readiness and vice versa.
[ ] Internal health check should be configurable:

- [ ] Period
- [ ] Timeout
- [ ] Allow externally inbound health checks to actually execute the health checks
- [ ] Allow the server to refuse starting unless all upstreams are healthy (configurable)

[ ] Health checks should run with a task manager
[ ] Add graceful shutdowns
[ ] Stop depending on the RUST_LOG environment variable. But do allow it to take effect if set. Add your defaults for which module gets which log level.
[ ] Forward request headers according to config
[ ] Readiness check should move upstreams with non-matching chain ids into the terminated list, and it should not check them again.
[ ] When a block is received as a response, populate the cache for all different kinds of requests that can result in that block. For example, a block with "latest" could have been received. So you can populate block by hash, block by number, etc.
[ ] Do the same thing for transactions. If a block is received with a list of transactions, populate the cache for all different kinds of requests that can result in those transactions.
[ ] Create a method_filter that allows us to hardcode responses for certain methods. For example, eth_chainId or EthSignTypedData, EthSignTypedDataV3, EthSignTypedDataV4, etc.
[ ] Add dynamic health check updates - if an upstream returns unexpected results, remove it from the list of healthy upstreams and try to reconnect.
[ ] Don't evict blocks when their block number is not low-enough. Keep them in there. And then eventually double-check that they are still part of the canonical chain, after which you can cache them for a longer time.
[ ] Return metadata in response headers. For example: cached: true, upstream: http://localhost:8545 etc
[ ] Cache should actually update the local latest block number.
[ ] Add cluster mode to redis cache.
[ ] Add method whitelisting and blacklisting.
[ ] Readiness probe should check that the cache is ready and accessible.
[ ] Cache should validate a 100% key match after getting the value via hash
[ ] Remove all anvil dependencies. Just keep the EthRequest type.
[ ] Allow the user to add "redis-cache-prefix" in the configs
[ ] Redis cache could use the entire request as the key, and just store the response value instead of storing the req-res pair.
[ ] Allow optionally whitelisting and blacklisting methods from caching.
[ ] Allow custom caching ttls for certain methods via config.
[ ] Why is "eth_getTransactionCount" never cached?
[ ] Should actix log the request and response bodies?
[ ] Why are request_body and response_body not showing up in the spans in the console logs?
[ ] Add a dry-run mode - don't just return data from the cache, but also send it to upstream and compare the results.
[ ] Why is redis memory not growing like crazy?
[ ] Why don't we just cache everything for 1 block_time minimum?
[ ] Add request coalescing via dashmap. Reuse cache ttls for coalescing.
