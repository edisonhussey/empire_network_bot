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

    def berimond_adi_to_cra_waiting_time(self):
        """Gap between the berimond ACI and the CRA that follows it.

        Berimond has no ADI (the pair is ACI -> CRA), so despite the name this is
        the ACI -> CRA spacing: a 2.5s baseline plus jitter, never below 2.5s.
        The CRA is still held to the global ``4 + jitter`` gate between attacks.
        """

        mean = 2.5 + random.random() * 0.8
        return max(2.5, np.random.normal(loc=mean, scale=0.55))

    def berimond_attack_cooldown_jitter(self):
        """Jitter added on top of the 4s global floor between two CRAs."""

        return random.uniform(0.15, 0.85)
