import asyncio
import os
import statistics
import time
from autobahn_client import Address, Autobahn
import matplotlib.pyplot as plt

from tests.util import (
    deserialize_time_message,
    send_time_messages,
    serialize_time_message,
)

SAMPLE_COUNT = 100
HOST = "localhost"
PORT = 8080
TOPIC = "test"
EXTRA_PAYLOAD = os.urandom(1024 * 1024 * 4)

client = Autobahn(address=Address(host=HOST, port=PORT))


async def main():
    await client.begin()
    times = await send_time_messages(
        client, EXTRA_PAYLOAD, TOPIC, number_of_messages=SAMPLE_COUNT, sleep=0.01
    )

    print(f"Average time: {sum(times) / len(times)} milliseconds")
    print(f"Median time: {sorted(times)[len(times) // 2]}")
    print(f"Min time: {min(times)} milliseconds")
    print(f"Max time: {max(times)} milliseconds")
    print(f"Standard deviation: {statistics.stdev(times)} milliseconds")

    plt.hist(times, bins=30)
    plt.xlabel("Time (ms)")
    plt.ylabel("Frequency")
    plt.title("Distribution of Message Latencies")
    plt.show()


if __name__ == "__main__":
    asyncio.run(main())
