# -*- coding: utf-8 -*-
import sys
sys.path.append('py_demo/src')
from ab_sign import ab_sign

query = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken="
ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"

result = ab_sign(query, ua)
print(f"Query: {query}")
print(f"UA: {ua}")
print(f"a_bogus: {result}")
