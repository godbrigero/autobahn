import asyncio
import os
import time
from autobahn_client import Address, Autobahn

from tests.util import deserialize_time_message, serialize_time_message


HOST = "localhost"
PORT = 8080
TOPIC = "test"

client = Autobahn(address=Address(host=HOST, port=PORT))


async def send_time_message(client: Autobahn, payload: bytes, topic: str):
    await client.publish(topic, serialize_time_message(payload))


async def send_100_time_messages(
    client: Autobahn, payload: bytes, topic: str, sleep: float | None = None
):
    total_number_of_messages = 0
    times = []

    async def callback(payload: bytes):
        nonlocal total_number_of_messages, times
        sent_time = deserialize_time_message(payload)
        current_time = time.time() * 1000
        if sent_time is not None:
            times.append(current_time - sent_time)
            total_number_of_messages += 1

    await client.begin()
    await client.subscribe(topic, callback)
    await asyncio.sleep(0.5)

    for _ in range(100):
        await send_time_message(client, payload, topic)
        if sleep is not None:
            await asyncio.sleep(sleep)

    while total_number_of_messages < 100:
        await asyncio.sleep(0.01)

    return times


async def test_time_operation_time_only():
    payload = os.urandom(1)
    all = await send_100_time_messages(client, payload, TOPIC)
    sum_all = sum(all)
    avg = sum_all / len(all)

    assert avg > 0
    assert avg <= 1


async def test_time_operation_time_and_payload():
    payload = os.urandom(1024 * 1024 * 4)
    all = await send_100_time_messages(client, payload, TOPIC)
    sum_all = sum(all)
    avg = sum_all / len(all)

    assert avg > 0
    assert avg <= 7
