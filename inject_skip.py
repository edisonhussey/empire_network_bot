# # # # from mitmproxy import http
# # # # from datetime import datetime
# # # # import os
# # # # import threading
# # # # import sys
# # # # import select

# # # # CAPTURE_FOLDER = "captures"
# # # # os.makedirs(CAPTURE_FOLDER, exist_ok=True)

# # # # current_file = None
# # # # last_dump = datetime.now()

# # # # class RBC_Skipper:
# # # #     def __init__(self):
# # # #         self.last_skip = 0

# # # #     def send_skip(self, x=644, y=639):
# # # #         if time.time() - self.last_skip < 5:
# # # #             print("⏳ Too soon - wait a few seconds")
# # # #             return
        
# # # #         payload = f'%xt%EmpireEx_19%msd%1%{{ "X":{x},"Y":{y},"MID":-1,"NID":-1,"MST":"MS2","KID":"0" }}%'
# # # #         print(f"🚀 [SKIP] Sending RBC skip to ({x},{y})")
# # # #         print(payload)
# # # #         print("---")
# # # #         self.last_skip = time.time()

# # # # skipper = RBC_Skipper()

# # # # def websocket_message(flow: http.HTTPFlow):
# # # #     global current_file, last_dump
    
# # # #     msg = flow.websocket.messages[-1]
# # # #     direction = "CLIENT →" if msg.from_client else "SERVER ←"
# # # #     ts = datetime.now()
    
# # # #     content = msg.content
# # # #     decoded = None
# # # #     if msg.is_text:
# # # #         decoded = content.decode('utf-8', errors='replace')
# # # #     else:
# # # #         try:
# # # #             decoded = content.decode('utf-8', errors='replace')
# # # #         except:
# # # #             decoded = content.hex()
    
# # # #     # New file every 10 seconds
# # # #     if current_file is None or (ts - last_dump).total_seconds() >= 10:
# # # #         if current_file:
# # # #             current_file.close()
# # # #         filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
# # # #         current_file = open(filename, "a", encoding="utf-8")
# # # #         last_dump = ts
# # # #         print(f"📁 New capture: {filename}")
    
# # # #     entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
# # # #     current_file.write(entry)
# # # #     current_file.flush()
    
# # # #     if "%xt%" in str(decoded) or "gaa" in str(decoded) or len(str(decoded)) > 50:
# # # #         print(f"\n🔥 GAME PACKET [{direction}]\n{decoded[:1200]}{'...' if len(str(decoded)) > 1200 else ''}")

# # # # def keyboard_listener():
# # # #     print("Press '1' to send RBC skip (to 644,639)")
# # # #     while True:
# # # #         if select.select([sys.stdin], [], [], 0.1)[0]:
# # # #             key = sys.stdin.read(1)
# # # #             if key == '1':
# # # #                 skipper.send_skip(644, 639)

# # # # def websocket_start(flow: http.HTTPFlow):
# # # #     print(f"✅ Connected: {flow.request.url}")

# # # # def done():
# # # #     if current_file:
# # # #         current_file.close()

# # # # addons = [keyboard_listener]  # This will run in background

# # # from mitmproxy import http
# # # from datetime import datetime
# # # import os
# # # import sys
# # # import time

# # # CAPTURE_FOLDER = "captures"
# # # os.makedirs(CAPTURE_FOLDER, exist_ok=True)

# # # current_file = None
# # # last_dump = datetime.now()

# # # class RBC_Skipper:
# # #     def __init__(self):
# # #         self.last_skip = 0

# # #     def send_skip(self):
# # #         if time.time() - self.last_skip < 5:
# # #             print("⏳ Too soon - wait 5 seconds")
# # #             return
        
# # #         payload = '%xt%EmpireEx_19%msd%1%{"X":644,"Y":639,"MID":-1,"NID":-1,"MST":"MS2","KID":"0"}%'
# # #         print(f"\n🚀 [SKIP] Sending RBC skip to (644,639)")
# # #         print(payload)
# # #         print("---")
# # #         self.last_skip = time.time()

# # # skipper = RBC_Skipper()

# # # def websocket_message(flow: http.HTTPFlow):
# # #     global current_file, last_dump
    
# # #     msg = flow.websocket.messages[-1]
# # #     direction = "CLIENT →" if msg.from_client else "SERVER ←"
# # #     ts = datetime.now()
    
# # #     content = msg.content
# # #     decoded = None
# # #     if msg.is_text:
# # #         decoded = content.decode('utf-8', errors='replace')
# # #     else:
# # #         try:
# # #             decoded = content.decode('utf-8', errors='replace')
# # #         except:
# # #             decoded = content.hex()
    
# # #     # New file every 10 seconds
# # #     if current_file is None or (ts - last_dump).total_seconds() >= 10:
# # #         if current_file:
# # #             current_file.close()
# # #         filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
# # #         current_file = open(filename, "a", encoding="utf-8")
# # #         last_dump = ts
# # #         print(f"📁 New capture: {filename}")
    
# # #     entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
# # #     current_file.write(entry)
# # #     current_file.flush()
    
# # #     if "%xt%" in str(decoded) or "gaa" in str(decoded) or len(str(decoded)) > 50:
# # #         print(f"\n🔥 GAME PACKET [{direction}]\n{decoded[:1200]}{'...' if len(str(decoded)) > 1200 else ''}")

# # # def websocket_start(flow: http.HTTPFlow):
# # #     print(f"✅ Connected: {flow.request.url}")

# # # def done():
# # #     if current_file:
# # #         current_file.close()

# # # # This runs in the background for keyboard input
# # # import threading
# # # def keyboard_listener():
# # #     print("\nPress '1' to send RBC skip request")
# # #     while True:
# # #         try:
# # #             key = sys.stdin.read(1)
# # #             if key == '1':
# # #                 skipper.send_skip()
# # #         except:
# # #             pass

# # # # Start keyboard listener
# # # threading.Thread(target=keyboard_listener, daemon=True).start()

# # # addons = []


# # from mitmproxy import http
# # from datetime import datetime
# # import os
# # import time
# # import threading

# # CAPTURE_FOLDER = "captures"
# # os.makedirs(CAPTURE_FOLDER, exist_ok=True)

# # current_file = None
# # last_dump = datetime.now()

# # def send_rbc_skip():
# #     payload = '%xt%EmpireEx_19%msd%1%{"X":644,"Y":639,"MID":-1,"NID":-1,"MST":"MS2","KID":"0"}%'
# #     print(f"\n🚀 AUTO SKIP SENT at {datetime.now().strftime('%H:%M:%S')}")
# #     print(payload)
# #     print("---")

# # def auto_skip_loop():
# #     print("Auto RBC Skip started - sending immediately, then every 30 seconds...")
# #     while True:
# #         send_rbc_skip()
# #         time.sleep(30)

# # # Start auto skip immediately
# # threading.Thread(target=auto_skip_loop, daemon=True).start()

# # def websocket_message(flow: http.HTTPFlow):
# #     global current_file, last_dump
    
# #     msg = flow.websocket.messages[-1]
# #     direction = "CLIENT →" if msg.from_client else "SERVER ←"
# #     ts = datetime.now()
    
# #     content = msg.content
# #     decoded = None
# #     if msg.is_text:
# #         decoded = content.decode('utf-8', errors='replace')
# #     else:
# #         try:
# #             decoded = content.decode('utf-8', errors='replace')
# #         except:
# #             decoded = content.hex()
    
# #     if current_file is None or (ts - last_dump).total_seconds() >= 10:
# #         if current_file:
# #             current_file.close()
# #         filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
# #         current_file = open(filename, "a", encoding="utf-8")
# #         last_dump = ts
# #         print(f"📁 New capture: {filename}")
    
# #     entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
# #     current_file.write(entry)
# #     current_file.flush()
    
# #     if "%xt%" in str(decoded) or "gaa" in str(decoded) or len(str(decoded)) > 50:
# #         print(f"\n🔥 GAME PACKET [{direction}]\n{decoded[:1200]}{'...' if len(str(decoded)) > 1200 else ''}")

# # def websocket_start(flow: http.HTTPFlow):
# #     print(f"✅ Connected: {flow.request.url}")

# # def done():
# #     if current_file:
# #         current_file.close()

# # addons = []



# from mitmproxy import http
# from datetime import datetime
# import os
# import sys
# import threading
# import time

# CAPTURE_FOLDER = "captures"
# os.makedirs(CAPTURE_FOLDER, exist_ok=True)

# current_file = None
# last_dump = datetime.now()

# def websocket_message(flow: http.HTTPFlow):
#     global current_file, last_dump
    
#     msg = flow.websocket.messages[-1]
#     direction = "CLIENT →" if msg.from_client else "SERVER ←"
#     ts = datetime.now()
    
#     content = msg.content
#     decoded = None
#     if msg.is_text:
#         decoded = content.decode('utf-8', errors='replace')
#     else:
#         try:
#             decoded = content.decode('utf-8', errors='replace')
#         except:
#             decoded = content.hex()
    
#     if current_file is None or (ts - last_dump).total_seconds() >= 10:
#         if current_file:
#             current_file.close()
#         filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
#         current_file = open(filename, "a", encoding="utf-8")
#         last_dump = ts
#         print(f"📁 New capture: {filename}")
    
#     entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
#     current_file.write(entry)
#     current_file.flush()
    
#     if "%xt%" in str(decoded) or "gaa" in str(decoded) or len(str(decoded)) > 50:
#         print(f"\n🔥 GAME PACKET [{direction}]\n{decoded[:1200]}{'...' if len(str(decoded)) > 1200 else ''}")

# def websocket_start(flow: http.HTTPFlow):
#     print(f"✅ Connected: {flow.request.url}")

# def done():
#     if current_file:
#         current_file.close()

# # Live terminal input for sending packets
# def terminal_sender():
#     print("\n" + "="*70)
#     print("Live Packet Sender")
#     print("Type the full %xt%... packet and press ENTER to send")
#     print("Example: %xt%EmpireEx_19%msd%1%{\"X\":644,\"Y\":639,\"MID\":-1,...}%")
#     print("="*70 + "\n")
    
#     while True:
#         try:
#             line = input("SEND> ").strip()
#             if line:
#                 print(f"Sending: {line[:100]}...")
#                 # This is where you would send it through the websocket
#                 # For now it just logs it
#                 print("Packet queued for sending (add websocket send code here)")
#         except (EOFError, KeyboardInterrupt):
#             break

# # Start terminal sender in background
# threading.Thread(target=terminal_sender, daemon=True).start()

# addons = []


from mitmproxy import http
from datetime import datetime
import os
import sys
import threading
import time

CAPTURE_FOLDER = "captures"
os.makedirs(CAPTURE_FOLDER, exist_ok=True)

current_file = None
last_dump = datetime.now()

def websocket_message(flow: http.HTTPFlow):
    global current_file, last_dump
    
    msg = flow.websocket.messages[-1]
    direction = "CLIENT →" if msg.from_client else "SERVER ←"
    ts = datetime.now()
    
    content = msg.content
    decoded = None
    if msg.is_text:
        decoded = content.decode('utf-8', errors='replace')
    else:
        try:
            decoded = content.decode('utf-8', errors='replace')
        except:
            decoded = content.hex()
    
    # File logging (background)
    if current_file is None or (ts - last_dump).total_seconds() >= 10:
        if current_file:
            current_file.close()
        filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
        current_file = open(filename, "a", encoding="utf-8")
        last_dump = ts
        print(f"📁 New capture file: {filename}")   # Only this prints
    
    entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
    current_file.write(entry)
    current_file.flush()

def websocket_start(flow: http.HTTPFlow):
    print(f"✅ Connected: {flow.request.url}")

def done():
    if current_file:
        current_file.close()

# Live Terminal Sender (clean)
def terminal_sender():
    print("\n" + "="*70)
    print("Live Packet Sender - Type full %xt% packet and press ENTER")
    print("Example: %xt%EmpireEx_19%msd%1%{\"X\":644,\"Y\":639,\"MST\":\"MS2\"}%")
    print("="*70 + "\n")
    
    while True:
        try:
            line = input("SEND> ").strip()
            if line and line.startswith('%xt%'):
                print(f"→ Sending: {line[:120]}...")
                # Add actual sending code here later
            elif line:
                print("Packet must start with %xt%")
        except (EOFError, KeyboardInterrupt):
            break
        except Exception as e:
            print(f"Error: {e}")

threading.Thread(target=terminal_sender, daemon=True).start()

addons = []