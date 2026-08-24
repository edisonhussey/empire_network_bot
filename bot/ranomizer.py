import random
import numpy as np

class Randomizer:
    def __init__(self):
        return
    
    
    # def attack_interval():
        
    def adi_waiting_time(self):
        mean = 3.2 + (2 * random.random()) ** (1/2)
        return max(3.3, np.random.normal(loc = mean, scale = 2))

    def gaa_waiting_time(self):
        mean = 12.0 + random.random() * 9.0
        return max(8.0, np.random.normal(loc=mean, scale=3.5))

    def attack_send_waiting_time(self):
        mean = 7.0 + random.random() * 5.0
        return max(5.0, np.random.normal(loc=mean, scale=2.0))
