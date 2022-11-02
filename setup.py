#!/usr/bin/env python

from setuptools import setup, find_packages
from os import path

with open(path.join(path.abspath(path.dirname(__file__)), 'requirements.txt'), 'r') as f:
    requirements = f.read().splitlines()

setup(name='hftbacktest',
      version='1.0',
      license='MIT',
      description='High-frequency trading backtesting tool',
      keywords='high-frequency trading backtest',
      author='nkaz',
      author_email='nkaz001@protonmail.com',
      url='https://github.com/nkaz001/hftbacktest',
      install_requires=requirements,
      packages=find_packages(),
)
