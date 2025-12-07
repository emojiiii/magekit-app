# -*- coding: utf-8 -*-
import sys
sys.path.insert(0, r'E:\emojiiii\magekit-app\py_demo\src')

import asyncio
from src import spider

async def test():
    url = "https://live.douyin.com/313899056971"
    print(f"测试 URL: {url}")
    try:
        result = await spider.get_douyin_web_stream_data(url)
        print(f"结果: {result}")
        print(f"主播名: {result.get('anchor_name', 'N/A')}")
        print(f"状态: {result.get('status', 'N/A')}")
    except Exception as e:
        print(f"错误: {e}")

if __name__ == "__main__":
    asyncio.run(test())
