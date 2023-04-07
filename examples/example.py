from numba import njit
import pandas as pd

from hftbacktest import NONE, NEW, HftBacktest, GTX, BUY, SELL, Linear, IntpOrderLatency, SquareProbQueueModel


@njit
def market_making_algo(hbt):
    while hbt.run:
        # in microseconds
        if not hbt.elapse(0.1 * 1e6):
            return False
        hbt.clear_inactive_orders()

        """
        You can find the core ideas from the following articles.
        https://ieor.columbia.edu/files/seasdepts/industrial-engineering-operations-research/pdf-files/Borden_D_FESeminar_Sp10.pdf (page 5)
        https://arxiv.org/abs/1105.3115 (the last three equations on page 13 and 7 Backtests)
        https://blog.bitmex.com/wp-content/uploads/2019/11/Algo-Trading-and-Market-Making.pdf
        https://www.wikijob.co.uk/trading/forex/market-making

        Also see my other repo.
        """
        a = 1
        b = 1
        c = 1
        hs = 1

        # alpha, it can be a combination of several indicators.
        forecast = 0
        # in hft, it could be a measurement of short-term market movement such as high - low of the last x-min.
        volatility = 0
        # delta risk, it also can be a combination of several risks.
        risk = (c + volatility) * hbt.position
        half_spread = (c + volatility) * hs

        max_notional_position = 1000
        notional_qty = 100

        mid = (hbt.best_bid + hbt.best_ask) / 2.0

        # fair value pricing = mid + a * forecast
        #                      or underlying(correlated asset) + adjustment(basis + cost + etc) + a * forecast
        # risk skewing = -b * risk
        new_bid = mid + a * forecast - b * risk - half_spread
        new_ask = mid + a * forecast - b * risk + half_spread

        new_bid_tick = round(new_bid / hbt.tick_size)
        new_ask_tick = round(new_ask / hbt.tick_size)

        new_bid = new_bid_tick * hbt.tick_size
        new_ask = new_ask_tick * hbt.tick_size
        order_qty = round(notional_qty / mid / hbt.lot_size) * hbt.lot_size

        # Elapse a process time
        if not hbt.elapse(.05 * 1e6):
            return False

        last_order_id = -1
        update_bid = True
        update_ask = True
        for order in hbt.orders.values():
            if order.side == BUY:
                if round(order.price / hbt.tick_size) == new_bid_tick \
                        or hbt.position * mid > max_notional_position:
                    update_bid = False
                elif order.cancellable or hbt.position * mid > max_notional_position:
                    hbt.cancel(order.order_id)
                    last_order_id = order.order_id
            if order.side == SELL:
                if round(order.price / hbt.tick_size) == new_ask_tick \
                        or hbt.position * mid < -max_notional_position:
                    update_ask = False
                if order.cancellable or hbt.position * mid < -max_notional_position:
                    hbt.cancel(order.order_id)
                    last_order_id = order.order_id

        # It can be combined with grid trading strategy by sumitting multiple orders to capture the better spread.
        # Then, it needs a more sophiscated logic to efficiently maintain resting orders in the book.
        if update_bid:
            # There is only one order on a given price, use new_bid_tick as order Id.
            hbt.submit_buy_order(new_bid_tick, new_bid, order_qty, GTX)
            last_order_id = new_bid_tick
        if update_ask:
            # There is only one order on a given price, use new_ask_tick as order Id.
            hbt.submit_sell_order(new_ask_tick, new_ask, order_qty, GTX)
            last_order_id = new_ask_tick

        # All order requests are considered to be requested at the same time.
        # Wait until one of the order responses is received.
        if last_order_id >= 0:
            if not hbt.wait_order_response(last_order_id):
                return False

        print(hbt.local_timestamp, mid, hbt.position, hbt.position * mid + hbt.balance - hbt.fee)
    return True


if __name__ == '__main__':
    # data file
    # https://github.com/nkaz001/collect-binancefutures

    # This backtest assumes market maker rebates.
    # https://www.binance.com/kz/support/announcement/binance-upgrades-usd%E2%93%A2-margined-futures-liquidity-provider-program-2023-04-04-01007356e6514df3811b0c80ab8c83bf

    latency_data1 = np.load('order_latency_20220831.npz')['data']
    latency_data2 = np.load('order_latency_20220901.npz')['data']
    latency_data = np.concatenate([latency_data1, latency_data2], axis=0)

    hbt = HftBacktest(
        [
            '../../btcusdt_20220831.npz',
            '../../btcusdt_20220901.npz'
        ],
        tick_size=0.1,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=IntpOrderLatency(latency_data),
        queue_model=SquareProbQueueModel(),
        asset_type=Linear,
        snapshot='../../btcusdt_20220830_eod.npz'
    )
    market_making_algo(hbt)

