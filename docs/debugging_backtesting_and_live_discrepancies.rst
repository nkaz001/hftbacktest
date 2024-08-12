Debugging Backtesting and Live Discrepancies
============================================

Plotting both live and backtesting values on a single chart is a good initial step. It's strongly recommended to include
the equity curve and position plots for comparison purposes. Additionally, visualizing your alpha, order prices, etc can
facilitate the identification of discrepancies.

[Image]

If the backtested strategy is correctly implemented in live trading, two significant factors may contribute to any
observed discrepancies.

1. Latency:
Latency, encompassing both feed and order latency, plays a crucial role in ensuring accurate backtesting results. It's
highly recommended to collect data yourself to accurately measure feed latency on your end. Alternatively, if obtaining
data from external sources, it's essential to verify that the feed latency aligns with your latency.

Order latency, measured from your end, can be collected by logging order actions or regularly submitting orders away
from the mid-price and subsequently canceling them to measure and record order latency.

It's still possible to artificially decrease latencies to assess improvements in strategy performance due to enhanced
latency. This allows you to evaluate the effectiveness of higher-tier programs or liquidity provider programs, as well
as quantify the impact of investments made in infrastructure improvement. Understanding whether a superior
infrastructure provides a competitive advantage is beneficial.

2. Queue Model:
Selecting an appropriate queue model that accurately reflects live trading results is essential. You can either develop
your own queue model or utilize existing ones. Hftbacktest offers three primary queue models such as
``PowerProbQueueModel`` series, allowing for adjustments to align with your results. For further information, refer to
:ref:`ProbQueueModel <order_fill_prob_queue_model>`.

One crucial point to bear in mind is the backtesting conducted under the assumption of no market impact. A market order,
or a limit order that take liquidity, can introduce discrepancies, as it may cause market impact and consequently make
execution simulation difficult. Moreover, if your limit order size is too large, partial fills and their market impact
can also lead to discrepancies. It's advisable to begin trading with a small size and align the results first. Gradually
increasing your trading size while observing both live and backtesting results is recommended.