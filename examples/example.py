import numpy as np

from numba import njit

from hftbacktest import BacktestAsset, HashMapMarketDepthBacktest, BUY, SELL, GTX, LIMIT


@njit
def market_making_algo(hbt):
    asset_no = 0
    tick_size = hbt.depth(asset_no).tick_size
    lot_size = hbt.depth(asset_no).lot_size

    # in nanoseconds
    while hbt.elapse(10_000_000) == 0:
        hbt.clear_inactive_orders(asset_no)

        """
        You can find the core ideas from the following articles.
        https://ieor.columbia.edu/files/seasdepts/industrial-engineering-operations-research/pdf-files/Borden_D_FESeminar_Sp10.pdf (page 5)
        https://arxiv.org/abs/1105.3115 (the last three equations on page 13 and 7 Backtests)
        https://blog.bitmex.com/wp-content/uploads/2019/11/Algo-Trading-and-Market-Making.pdf

        Also see my other repo.
        """
        a = 1
        b = 1
        c = 1
        hs = 1

        # Alpha, it can be a combination of several indicators.
        forecast = 0
        # In HFT, it can be various measurements of short-term market movements,
        # such as the high-low range in the last X minutes.
        volatility = 0
        # Delta risk, it can be a combination of several risks.
        position = hbt.position(asset_no)
        risk = (c + volatility) * position
        half_spread = (c + volatility) * hs

        max_notional_position = 1000
        notional_qty = 100

        depth = hbt.depth(asset_no)

        mid_price = (depth.best_bid + depth.best_ask) / 2.0

        # fair value pricing = mid_price + a * forecast
        #                      or underlying(correlated asset) + adjustment(basis + cost + etc) + a * forecast
        # risk skewing = -b * risk
        reservation_price = mid_price + a * forecast - b * risk
        new_bid = reservation_price - half_spread
        new_ask = reservation_price + half_spread

        new_bid_tick = min(np.round(new_bid / tick_size), depth.best_bid_tick)
        new_ask_tick = max(np.round(new_ask / tick_size), depth.best_ask_tick)

        order_qty = np.round(notional_qty / mid_price / lot_size) * lot_size

        # Elapses a process time.
        if not hbt.elapse(1_000_000) != 0:
            return False

        last_order_id = -1
        update_bid = True
        update_ask = True
        buy_limit_exceeded = position * mid_price > max_notional_position
        sell_limit_exceeded = position * mid_price < -max_notional_position
        orders = hbt.orders(asset_no)
        order_values = orders.values()
        while order_values.has_next():
            order = order_values.get()
            if order.side == BUY:
                if order.price_tick == new_bid_tick or buy_limit_exceeded:
                    update_bid = False
                if order.cancellable and (update_bid or buy_limit_exceeded):
                    hbt.cancel(asset_no, order.order_id, False)
                    last_order_id = order.order_id
            elif order.side == SELL:
                if order.price_tick == new_ask_tick or sell_limit_exceeded:
                    update_ask = False
                if order.cancellable and (update_ask or sell_limit_exceeded):
                    hbt.cancel(asset_no, order.order_id, False)
                    last_order_id = order.order_id

        # It can be combined with a grid trading strategy by submitting multiple orders to capture better spreads and
        # have queue position.
        # This approach requires more sophisticated logic to efficiently manage resting orders in the order book.
        if update_bid:
            # There is only one order at a given price, with new_bid_tick used as the order ID.
            order_id = new_bid_tick
            hbt.submit_buy_order(asset_no, order_id, new_bid_tick * tick_size, order_qty, GTX, LIMIT, False)
            last_order_id = order_id
        if update_ask:
            # There is only one order at a given price, with new_ask_tick used as the order ID.
            order_id = new_ask_tick
            hbt.submit_sell_order(asset_no, order_id, new_ask_tick * tick_size, order_qty, GTX, LIMIT, False)
            last_order_id = order_id

        # All order requests are considered to be requested at the same time.
        # Waits until one of the order responses is received.
        if last_order_id >= 0:
            # Waits for the order response for a maximum of 5 seconds.
            timeout = 5_000_000_000
            if not hbt.wait_order_response(asset_no, last_order_id, timeout):
                return False

    return True


if __name__ == '__main__':
    # This backtest assumes market maker rebates.
    # https://www.binance.com/en/support/announcement/binance-upgrades-usd%E2%93%A2-margined-futures-liquidity-provider-program-2023-04-04-01007356e6514df3811b0c80ab8c83bf
    asset = (
        BacktestAsset()
            .data([
                'data/btcusdt_20220831.npz',
                'data/btcusdt_20220901.npz',
            ])
            .initial_snapshot('data/btcusdt_20220830_eod.npz')
            .linear_asset(1.0)
            .intp_order_latency([
                'latency/live_order_latency_20220831.npz',
                'latency/live_order_latency_20220901.npz',
            ])
            .power_prob_queue_model(2.0)
            .no_partial_fill_exchange()
            .trading_value_fee_model(-0.00005, 0.0007)
            .tick_size(0.1)
            .lot_size(0.001)
    )
    hbt = HashMapMarketDepthBacktest([asset])
    market_making_algo(hbt)
