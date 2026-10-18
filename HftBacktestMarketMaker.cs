using System;
using System.Linq;
using cAlgo.API;

namespace cAlgo.Robots
{
    // Ports the README quick-start pricing and order-management behavior, not HftBacktest's simulator.
    // cTrader quotes use Symbol.Bid/Ask and do not model HftBacktest feed/order latency or queue position.
    // PlaceLimitOrder has no GTX/post-only option; top-of-book price caps reduce crossing, but cannot prevent
    // execution if the market moves between reading the quote and the broker receiving the order.
    [Robot(TimeZone = TimeZones.UTC, AccessRights = AccessRights.None)]
    public class HftBacktestMarketMaker : Robot
    {
        [Parameter("Half-spread (pips)", DefaultValue = 1.0, MinValue = 0.0)]
        public double HalfSpreadPips { get; set; }

        [Parameter("Order notional (quote currency)", DefaultValue = 100.0, MinValue = 0.0)]
        public double OrderNotional { get; set; }

        [Parameter("Maximum absolute position (quote currency)", DefaultValue = 1000.0, MinValue = 0.0)]
        public double MaxNotionalPosition { get; set; }

        [Parameter("Inventory skew at position cap (pips)", DefaultValue = 1.0, MinValue = 0.0)]
        public double InventorySkewAtPositionCapPips { get; set; }

        [Parameter("Order label (unique per symbol)", DefaultValue = "HftBacktestMarketMaker")]
        public string OrderLabel { get; set; }

        private bool _volumeWarningPrinted;

        protected override void OnStart()
        {
            if (OrderNotional <= 0 || MaxNotionalPosition <= 0 || string.IsNullOrWhiteSpace(OrderLabel))
            {
                Print("Order notional, maximum position, and order label must be valid and non-zero.");
                Stop();
            }
        }

        protected override void OnTick()
        {
            var bid = Symbol.Bid;
            var ask = Symbol.Ask;
            if (bid <= 0 || ask <= bid)
            {
                CancelStrategyOrders();
                return;
            }

            var midPrice = (bid + ask) / 2.0;
            var inventoryUnits = Positions
                .Where(position => position.SymbolName == SymbolName && position.Label == OrderLabel)
                .Sum(position => position.TradeType == TradeType.Buy
                    ? position.VolumeInUnits
                    : -position.VolumeInUnits);
            var inventoryNotional = inventoryUnits * midPrice;

            // The README example has zero forecast and volatility. Normalize its inventory-risk
            // coefficient to the configured position cap for symbols with different price scales.
            var reservationPrice = midPrice
                - (inventoryNotional / MaxNotionalPosition)
                * InventorySkewAtPositionCapPips
                * Symbol.PipSize;
            var halfSpread = HalfSpreadPips * Symbol.PipSize;

            var newBid = FloorToTick(Math.Min(reservationPrice - halfSpread, bid));
            var newAsk = CeilingToTick(Math.Max(reservationPrice + halfSpread, ask));
            var orderVolume = Symbol.NormalizeVolumeInUnits(OrderNotional / midPrice, RoundingMode.Down);

            if (newBid <= 0 || newAsk <= newBid)
            {
                CancelStrategyOrders();
                return;
            }

            if (orderVolume < Symbol.VolumeInUnitsMin)
            {
                if (!_volumeWarningPrinted)
                {
                    Print("Order notional produces volume {0}, below this symbol's minimum volume {1}; no quotes will be placed.",
                        orderVolume, Symbol.VolumeInUnitsMin);
                    _volumeWarningPrinted = true;
                }

                CancelStrategyOrders();
                return;
            }

            var buyLimitExceeded = inventoryNotional > MaxNotionalPosition;
            var sellLimitExceeded = inventoryNotional < -MaxNotionalPosition;
            MaintainQuote(TradeType.Buy, !buyLimitExceeded, newBid, orderVolume);
            MaintainQuote(TradeType.Sell, !sellLimitExceeded, newAsk, orderVolume);
        }

        protected override void OnStop()
        {
            CancelStrategyOrders();
        }

        private void MaintainQuote(TradeType side, bool enabled, double targetPrice, double volume)
        {
            var orders = PendingOrders
                .Where(order => order.SymbolName == SymbolName
                    && order.Label == OrderLabel
                    && order.OrderType == PendingOrderType.Limit
                    && order.TradeType == side)
                .ToArray();

            if (!enabled)
            {
                CancelOrders(orders);
                return;
            }

            var matchingOrder = orders.FirstOrDefault(order =>
                Math.Abs(order.TargetPrice - targetPrice) < Symbol.TickSize / 2.0
                && Math.Abs(order.VolumeInUnits - volume) < Symbol.VolumeInUnitsStep / 2.0);
            if (matchingOrder != null)
            {
                CancelOrders(orders.Where(order => order.Id != matchingOrder.Id).ToArray());
                return;
            }

            if (!CancelOrders(orders))
                return;

            var result = PlaceLimitOrder(side, SymbolName, volume, targetPrice, OrderLabel);
            if (!result.IsSuccessful)
                Print("Could not place {0} limit order at {1}: {2}", side, targetPrice, result.Error);
        }

        private double FloorToTick(double price)
        {
            return Math.Floor(price / Symbol.TickSize + 1e-10) * Symbol.TickSize;
        }

        private double CeilingToTick(double price)
        {
            return Math.Ceiling(price / Symbol.TickSize - 1e-10) * Symbol.TickSize;
        }

        private void CancelStrategyOrders()
        {
            var orders = PendingOrders
                .Where(order => order.SymbolName == SymbolName
                    && order.Label == OrderLabel
                    && order.OrderType == PendingOrderType.Limit)
                .ToArray();
            CancelOrders(orders);
        }

        private bool CancelOrders(PendingOrder[] orders)
        {
            var allCancelled = true;
            foreach (var order in orders)
            {
                var result = CancelPendingOrder(order);
                if (result.IsSuccessful)
                    continue;

                Print("Could not cancel pending order {0}: {1}", order.Id, result.Error);
                allCancelled = false;
            }

            return allCancelled;
        }
    }
}
