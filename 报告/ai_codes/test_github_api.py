import urllib.request
import json

url = "https://api.github.com/search/repositories?q=CARD+language:R&sort=stars"
req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
try:
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode('utf-8'))
        items = data.get('items', [])
        print(f"Total count: {data.get('total_count')}")
        print("Top results:")
        for item in items[:20]:
            print(f"- {item.get('full_name')} (stars: {item.get('stargazers_count')})")
except Exception as e:
    print(f"Error: {e}")
