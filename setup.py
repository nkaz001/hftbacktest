#!/usr/bin/env python

from setuptools import setup, find_packages
from os import path

with open(path.join(path.abspath(path.dirname(__file__)), 'requirements.txt'), 'r') as f:
    requirements = f.read().splitlines()

setup(name='hftbacktest',
      version='1.0',
      description='High frequency trading backtest tool',
      author='nkaz001@protonmail.com',
      author_email='nkaz001@protonmail.com',
      install_requires=requirements,
      packages=find_packages(),
)

