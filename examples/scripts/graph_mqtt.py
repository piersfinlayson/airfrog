#!/usr/bin/env python3
# graph_mqtt.py
#
# Designed to be used with the airfrog mqtt example.
#
# Graphs counter values received from one or more airfrogs using MQTT. 
#
# Dependencies:
# - paho-mqtt
# - matplotlib
#
# pip install paho-mqtt matplotlib 

import paho.mqtt.client as mqtt
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import matplotlib.dates as mdates
from collections import deque, defaultdict
import datetime

plt.style.use('dark_background')
plt.rcParams.update({
    'font.size': 12,
    'font.weight': 'bold',
    'axes.linewidth': 2,
    'axes.labelweight': 'bold',
    'xtick.major.width': 2,
    'ytick.major.width': 2,
    'xtick.labelsize': 11,
    'ytick.labelsize': 11,
    'grid.linewidth': 1.5
})

# Update MQTT_BROKER to your MQTT broker address
MQTT_BROKER = 'mosquitto'

# You can likely leave this as is, it subscribes to all airfrogs' counters
MQTT_TOPIC = 'airfrog/+/counter'

class MQTTCounterPlotter:
    def __init__(self, broker, topic, max_points=100):
        self.devices = defaultdict(lambda: {
            'times': deque(maxlen=max_points),
            'values': deque(maxlen=max_points)
        })
        self.colors = ['b-', 'r-', 'g-', 'm-', 'c-', 'y-', 'k-']
        
        self.client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)
        self.client.on_message = self.on_message
        self.client.connect(broker, 1883, 60)
        self.client.subscribe(topic)
        self.client.loop_start()
        
        self.fig, self.ax = plt.subplots(figsize=(12, 9))
        self.fig.canvas.manager.set_window_title('Airfrog MQTT')
        
    def on_message(self, client, userdata, msg, properties=None):
        try:
            device_id = msg.topic.split('/')[1]
            value = int(msg.payload.decode())
            now = datetime.datetime.now()
            
            self.devices[device_id]['times'].append(now)
            self.devices[device_id]['values'].append(value)
        except (ValueError, IndexError):
            pass
            
    def update_plot(self, frame):
        self.ax.clear()
        
        for i, (device_id, data) in enumerate(self.devices.items()):
            if data['times'] and data['values']:
                color = self.colors[i % len(self.colors)]
                self.ax.plot(data['times'], data['values'], color, 
                           label=f'{device_id} ROM', linewidth=2)
        
        self.ax.set_ylabel('ROM Accesses/sec', fontsize=16, fontweight='bold')
        self.ax.set_title('Software Defined Retro ROM Access Rate', fontsize=20, fontweight='bold')
        self.ax.set_ylim(0, 1000000)
        self.ax.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))
        self.ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda x, p: f'{int(x):,}'))
        self.ax.legend(loc='lower left')
        self.ax.grid(True, alpha=0.4, linewidth=1.5)
        plt.xticks(rotation=45)
        
        return []

# Usage
plotter = MQTTCounterPlotter(MQTT_BROKER, MQTT_TOPIC)
ani = animation.FuncAnimation(plotter.fig, plotter.update_plot, interval=1000, cache_frame_data=False)
plt.subplots_adjust(left=0.2, bottom=0.15)
plt.show()