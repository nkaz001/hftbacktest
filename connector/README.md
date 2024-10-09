# HftBacktest - Connector
Connector provides a single point of communication with exchanges, brokers, or data feed providers. 
It is designed to manage multiple bots, allowing each bot to connect to several different connectors simultaneously.

![architecture](https://github.com/nkaz001/hftbacktest/blob/master/docs/images/arch.png)

## Supported Exchanges
**CAUTION: Use at your own risk. Live trading features may not function correctly in all cases.
Please report any issues you encounter by submitting them to the Issues.**

Supported exchanges include:

* Binance Futures (Tested on the Testnet)
* Bybit Futures (Under development)

## Getting Started

1. Clone the repository:

    ```
    git clone https://github.com/nkaz001/hftbacktest.git
    ```

2. Build Connector. After building, the executable file `connector` will be generated under `target/release` directory:

    ```
    cargo build --release --package connector
    ```

3. Configure the settings file. Please see the [examples](https://github.com/nkaz001/hftbacktest/blob/master/connector/examples) directory for guidance.

4. Run Connector. You can run multiple instances of Connector for the same exchange using different names and configurations:

    **Example**
    ```
    connector --name bf --connector binancefutures --config binancefutures.toml
    ```

Note: Since Connector communicates with bots via shared memory, both Connector and the bots must run on the same machine.

## Connector Implementation Guide
If a connector adheres to the IPC protocol, it does not have to be implemented in the same manner as Connector.
However, following this implementation makes it easier to develop additional connectors.

To implement a connector, you mainly need to implement two traits: `Connector` and `ConnectorBuilder`.

For further details, please see the documentation.