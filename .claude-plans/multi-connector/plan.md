# Task: Multi-Connector Support for HFT Supervisor

## Goal
Enable the HFT supervisor to manage multiple connectors simultaneously (e.g., Binance Testnet + Bybit Testnet), with each bot able to specify which connector to use.

## Current State (AFTER IMPLEMENTATION)
- Supervisor supports both `[connector]` (legacy) and `[[connectors]]` (new) in cluster.toml
- `infra.rs` uses `HashMap<InfraComponent, ManagedComponent>` for flexible component management
- `InfraComponent::Connector(String)` identifies connectors by name
- Bots specify `connector = "name"` to select which connector to use
- `manager.rs` passes `--connector` argument when spawning bots
- `hftctl status` shows all connectors with their status

## Target State
- Support `[[connectors]]` array in cluster.toml with multiple connector definitions
- Each connector runs as separate process with its own config
- Bots specify `connector = "bf"` or `connector = "bybit"` to select connector
- Collector subscribes to first/default connector (or make configurable later)
- Status display shows all connectors

## Phases

- [x] Phase 1: Plan and analyze current code
- [x] Phase 2: Update supervisor code (config.rs, infra.rs, ipc.rs, hftctl.rs, manager.rs)
- [x] Phase 3: Update cluster.toml with [[connectors]] format
- [x] Phase 4: Rebuild Docker and test - VERIFIED WORKING

## Files to Modify

### Phase 2: Supervisor Code
1. `supervisor/src/config.rs` (DONE)
   - [x] Add `connectors: Vec<ConnectorConfig>` field
   - [x] Add `connector: Option<String>` to BotConfig
   - [x] Add helper methods: `all_connectors()`, `get_connector()`, `default_connector_name()`
   - [x] Update validation for multiple connectors

2. `supervisor/src/infra.rs` (DONE)
   - [x] Change `InfraComponent::Connector` to `InfraComponent::Connector(String)`
   - [x] Change `components` from fixed array to `HashMap<InfraComponent, ManagedComponent>`
   - [x] Update `start_component()` to handle named connectors
   - [x] Update `stop_component()` to handle named connectors
   - [x] Update `start_auto_start()` to start all connectors
   - [x] Update `stop_all()` to stop all connectors
   - [x] Update `monitor()` for HashMap iteration
   - [x] Update `handle_pending_restarts()` for HashMap access
   - [x] Update `spawn_component()` to get connector config by name
   - [x] Update `ClusterHealth` for multi-connector
   - [x] Fix tests for InfraComponent::Connector(String)
   - [x] Update `ipc.rs` for multi-connector StatusResponse
   - [x] Update `hftctl.rs` for multi-connector display

3. `supervisor/src/manager.rs` (DONE)
   - [x] Update bot spawn to pass connector name (`--connector` arg)
   - [x] Check calls to InfraManager methods

4. `supervisor/src/ipc.rs` (DONE)
   - [x] Update status commands for multi-connector display
   - [x] Add `ConnectorStatus` struct with name field
   - [x] Change `StatusResponse.connector` to `connectors: Vec<ConnectorStatus>`

### Phase 3: Configuration
1. `deploy/docker/config/cluster.toml` (DONE)
   - [x] Change `[connector]` to `[[connectors]]`
   - [x] Add Binance Futures testnet connector (name: bf)
   - [x] Add Bybit Demo Trading connector (name: bybit)
   - [x] Update bot configs with `connector = "..."` field

2. `deploy/docker/config/binancefutures.toml` (DONE)
   - [x] Fixed testnet URLs (REST: demo-fapi.binance.com, WS: fstream.binancefuture.com)

3. `deploy/docker/config/bybit.toml` (DONE - using Demo Trading)
   - [x] Configured for Demo Trading (REST API for orders)

4. `deploy/docker/entrypoint.sh` (DONE)
   - [x] Already handles both Binance and Bybit config files
   - [x] No changes needed

5. `deploy/docker/.env` (DONE)
   - [x] Has both Binance and Bybit API keys configured

## Key Questions
1. How should collector handle multiple connectors?
   - Answer: Use first/default connector for now
2. How to display multiple connector status in hftctl?
   - Answer: Show each connector as separate line (connector:bf, connector:bybit)

## Decisions Made
- Use `HashMap<InfraComponent, ManagedComponent>` for flexible component management
- `InfraComponent::Connector(String)` to identify connectors by name
- Backward compatible: support both `[connector]` (legacy) and `[[connectors]]` (new)
- Collector uses first connector's IPC (configurable in future)

## Errors Encountered
- Fixed: `config.connector` changed from `ConnectorConfig` to `Option<ConnectorConfig>` - tests needed updating
- Fixed: `InfraComponent::Connector` changed to `InfraComponent::Connector(String)` - all usages updated
- Fixed: HashMap iteration patterns differ from array - updated `stop_all`, `monitor`, `handle_pending_restarts`
- Fixed: Temporary value dropped while borrowed in `spawn_component` - bind to variable first

## Status
**COMPLETED** - Multi-connector support is working

Final results:
1. [x] Verify Binance testnet configuration (fixed URLs)
2. [x] Build Docker image with supervisor changes
3. [x] Start containers - both connectors running
4. [x] Verify connectors start correctly - orders submitted to both exchanges

Key fixes applied:
- Fixed Binance testnet URLs (REST: demo-fapi.binance.com, WS: fstream.binancefuture.com)
- Fixed symbol case (Binance needs lowercase, Bybit needs uppercase)
- Fixed supervisor to pass `--connector` argument to bots
