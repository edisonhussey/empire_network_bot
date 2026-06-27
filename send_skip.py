# import requests
# import time

# PROXY = "http://127.0.0.1:8080"

# def send_rbc_skip():
#     payload = '%xt%EmpireEx_19%msd%1%{"X":644,"Y":639,"MID":-1,"NID":-1,"MST":"MS2","KID":"0"}%'
    
#     headers = {
#         "Host": "game.goodgame.com",
#         "Content-Type": "text/plain",
#     }
    
#     try:
#         response = requests.post(
#             "http://game.goodgame.com",
#             data=payload,
#             headers=headers,
#             proxies={"http": PROXY, "https": PROXY},
#             timeout=10
#         )
#         print(f"[{time.strftime('%H:%M:%S')}] Sent skip | Status: {response.status_code}")
#     except Exception as e:
#         print(f"Error sending skip: {e}")

# if __name__ == "__main__":
#     print("RBC Skip Sender - Ctrl+C to stop")
#     while True:
#         send_rbc_skip()
#         time.sleep(8)  # Adjust as needed


import requests
import time

PROXY = "http://127.0.0.1:8080"

def send_skip():
    payload = '%xt%EmpireEx_19%msd%1%{"X":644,"Y":639,"MID":-1,"NID":-1,"MST":"MS2","KID":"0"}%'
    
    headers = {
        "Host": "game.goodgame.com",
        "Content-Type": "text/plain",
        "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"
    }
    
    try:
        response = requests.post(
            "http://game.goodgame.com",
            data=payload,
            headers=headers,
            proxies={"http": PROXY},
            timeout=10
        )
        print(f"[{time.strftime('%H:%M:%S')}] Skip sent | Status: {response.status_code}")
        if response.text:
            print("Response:", response.text[:200])
    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    print("RBC Skip Sender - Press Ctrl+C to stop")
    count = 0
    while True:
        send_skip()
        count += 1
        print(f"Sent {count} skips so far")
        time.sleep(8)  # Change to your desired interval
