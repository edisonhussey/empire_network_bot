# # from mitmproxy import http
# # import json
# # from datetime import datetime
# # import os

# # CAPTURE_FOLDER = "captures"
# # os.makedirs(CAPTURE_FOLDER, exist_ok=True)

# # current_file = None
# # packet_count = 0

# # def websocket_message(flow: http.HTTPFlow):
# #     global current_file, packet_count
    
# #     msg = flow.websocket.messages[-1]
# #     direction = "CLIENT" if msg.from_client else "SERVER"
# #     ts = datetime.now()
    
# #     content = msg.content
# #     if msg.is_text:
# #         try:
# #             content = content.decode('utf-8', errors='replace')
# #         except:
# #             content = str(content)
    
# #     # Create new file every 30 seconds or on first message
# #     if current_file is None or (ts.minute * 60 + ts.second) % 30 == 0:
# #         filename = f"{CAPTURE_FOLDER}/gge_capture_{ts.strftime('%Y%m%d_%H%M%S')}.log"
# #         current_file = open(filename, "a", encoding="utf-8")
# #         print(f"📁 New capture file: {filename}")
    
# #     # Write the packet
# #     entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction} | {len(content)} chars\n{content}\n---\n"
# #     current_file.write(entry)
# #     current_file.flush()
    
# #     packet_count += 1
# #     if packet_count % 20 == 0:
# #         print(f"📊 Captured {packet_count} packets so far...")

# # def websocket_start(flow: http.HTTPFlow):
# #     print(f"✅ WebSocket connected: {flow.request.url}")

# # def done():
# #     if current_file:
# #         current_file.close()


# from mitmproxy import http
# from datetime import datetime
# import os

# CAPTURE_FOLDER = "captures"
# os.makedirs(CAPTURE_FOLDER, exist_ok=True)

# current_file = None
# last_dump = datetime.now()

# def websocket_message(flow: http.HTTPFlow):
#     global current_file, last_dump
    
#     msg = flow.websocket.messages[-1]
#     direction = "CLIENT →" if msg.from_client else "SERVER ←"
#     ts = datetime.now()
    
#     # Get content safely
#     content = msg.content
#     if msg.is_text:
#         try:
#             content = content.decode('utf-8', errors='replace')
#         except:
#             content = str(content)
#     else:
#         content = content.hex()[:300] + "..." if len(content) > 300 else content.hex()
    
#     # New file every 10 seconds
#     if current_file is None or (ts - last_dump).total_seconds() >= 10:
#         if current_file:
#             current_file.close()
#         filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
#         current_file = open(filename, "a", encoding="utf-8")
#         last_dump = ts
#         print(f"📁 New capture file: {filename}")
    
#     entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{content}\n---\n"
#     current_file.write(entry)
#     current_file.flush()
    
#     # Highlight game packets in terminal
#     if "%xt%" in str(content) or "gaa" in str(content):
#         print(f"\n🔥 GAME PACKET [{direction}]\n{content[:1000]}{'...' if len(str(content)) > 1000 else ''}")

# def websocket_start(flow: http.HTTPFlow):
#     print(f"✅ WebSocket connected: {flow.request.url}")

# def done():
#     if current_file:
#         current_file.close()


from mitmproxy import http
from datetime import datetime
import os

CAPTURE_FOLDER = "captures"
os.makedirs(CAPTURE_FOLDER, exist_ok=True)

current_file = None
last_dump = datetime.now()

def websocket_message(flow: http.HTTPFlow):
    global current_file, last_dump
    
    msg = flow.websocket.messages[-1]
    direction = "CLIENT →" if msg.from_client else "SERVER ←"
    ts = datetime.now()
    
    # Try to decode as UTF-8, fallback to hex
    content = msg.content
    decoded = None
    if msg.is_text:
        decoded = content.decode('utf-8', errors='replace')
    else:
        try:
            decoded = content.decode('utf-8', errors='replace')
        except:
            decoded = content.hex()
    
    # New file every 10 seconds
    if current_file is None or (ts - last_dump).total_seconds() >= 10:
        if current_file:
            current_file.close()
        filename = f"{CAPTURE_FOLDER}/gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
        current_file = open(filename, "a", encoding="utf-8")
        last_dump = ts
        print(f"📁 New capture: {filename}")
    
    entry = f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{decoded}\n---\n"
    current_file.write(entry)
    current_file.flush()
    
    # Show meaningful game packets in terminal
    if "%xt%" in str(decoded) or "gaa" in str(decoded) or len(str(decoded)) > 50:
        print(f"\n🔥 GAME PACKET [{direction}]\n{decoded[:1200]}{'...' if len(str(decoded)) > 1200 else ''}")

def websocket_start(flow: http.HTTPFlow):
    print(f"✅ Connected: {flow.request.url}")

def done():
    if current_file:
        current_file.close()
