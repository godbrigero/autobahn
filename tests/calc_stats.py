import asyncio
import os
import statistics
import time
from autobahn_client import Address, Autobahn
import matplotlib.pyplot as plt

from util import deserialize_time_message, serialize_time_message

SAMPLE_COUNT = 100
HOST = "localhost"
PORT = 8080
TOPIC = "test"

client = Autobahn(address=Address(host=HOST, port=PORT))


async def main():
    times: list[float] = []
    total_number_of_messages = 0

    async def callback(payload: bytes):
        nonlocal total_number_of_messages, times
        sent_time = deserialize_time_message(payload)
        current_time = time.time() * 1000
        if sent_time is not None:
            times.append(current_time - sent_time)
            total_number_of_messages += 1

    await client.begin()
    await client.subscribe(TOPIC, callback)
    await asyncio.sleep(0.5)

    # 4 mb of random bytes
    payload = os.urandom(1024 * 1024 * 4)

    for _ in range(SAMPLE_COUNT):
        await client.publish(TOPIC, serialize_time_message(payload))
        await asyncio.sleep(0.01)

    while total_number_of_messages < SAMPLE_COUNT:
        await asyncio.sleep(0.01)

    print(f"Total number of messages: {total_number_of_messages}")

    if times:
        print(f"Average time: {sum(times) / len(times)} milliseconds")
        print(f"Median time: {sorted(times)[len(times) // 2]}")
        print(f"Min time: {min(times)} milliseconds")
        print(f"Max time: {max(times)} milliseconds")
        if len(times) > 1:
            print(f"Standard deviation: {statistics.stdev(times)} milliseconds")
        else:
            print("Standard deviation: 0.0 milliseconds")
    else:
        print("No timing samples captured.")

    plt.hist(times, bins=30)
    plt.xlabel("Time (ms)")
    plt.ylabel("Frequency")
    plt.title("Distribution of Message Latencies")
    plt.show()


if __name__ == "__main__":
    asyncio.run(main())
