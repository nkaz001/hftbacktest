#!/usr/bin/env python

from setuptools import setup, find_packages

setup(name='hftbacktest',
      version='1.0.2',
      license='MIT',
      description='High-frequency trading backtesting tool',
      keywords='high-frequency trading backtest',
      author='nkaz',
      author_email='nkaz001@protonmail.com',
      url='https://github.com/nkaz001/hftbacktest',
      install_requires=[
            'numba~=0.56',
            'numpy~=1.23',
            'pandas',
            'matplotlib',
      ],
      packages=find_packages(),
)
