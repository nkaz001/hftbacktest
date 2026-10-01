from unittest.mock import MagicMock

import numpy as np
import pytest

from hftbacktest.data.utils import snapshot as snapshot_module


@pytest.fixture
def snapshot_mocks(monkeypatch):
    asset = MagicMock()
    asset.data.return_value = asset
    asset.tick_size.return_value = asset
    asset.lot_size.return_value = asset
    monkeypatch.setattr(snapshot_module, 'BacktestAsset', MagicMock(return_value=asset))

    hbt = MagicMock()
    hbt._goto_end.return_value = 0
    hbt.depth.return_value.snapshot.return_value = np.array([(1.0, 2.0)])
    monkeypatch.setattr(snapshot_module, 'HashMapMarketDepthBacktest', MagicMock(return_value=hbt))
    return hbt


def test_create_last_snapshot_closes_backtest_on_success(snapshot_mocks):
    snapshot_module.create_last_snapshot(['data.npz'], 0.01, 0.001)

    snapshot_mocks.close.assert_called_once_with()


def test_create_last_snapshot_closes_backtest_on_goto_error(snapshot_mocks):
    snapshot_mocks._goto_end.return_value = 2

    with pytest.raises(RuntimeError):
        snapshot_module.create_last_snapshot(['data.npz'], 0.01, 0.001)

    snapshot_mocks.close.assert_called_once_with()


def test_create_last_snapshot_frees_snapshot_and_closes_on_copy_error(snapshot_mocks):
    snapshot = MagicMock()
    snapshot.copy.side_effect = ValueError('copy failed')
    snapshot_mocks.depth.return_value.snapshot.return_value = snapshot

    with pytest.raises(ValueError, match='copy failed'):
        snapshot_module.create_last_snapshot(['data.npz'], 0.01, 0.001)

    snapshot_mocks.depth.return_value.snapshot_free.assert_called_once_with(snapshot)
    snapshot_mocks.close.assert_called_once_with()
