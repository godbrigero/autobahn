import asyncio
import time

from autobahn_client import Autobahn


def serialize_time_message(payload: bytes):
    """
    Prepends the current time in milliseconds (as a float string, fixed width) to the payload.
    Returns a bytes object: b"<timestamp>|<payload>"
    """
    timestamp = f"{time.time() * 1000:.3f}"
    return timestamp.encode("utf-8") + b"|" + payload


def deserialize_time_message(payload: bytes):
    """
    Extracts the timestamp from the payload and returns it as a float.
    Assumes the payload is b"<timestamp>|<payload>"
    """
    try:
        sep_index = payload.find(b"|")
        if sep_index == -1:
            return None
        timestamp_bytes = payload[:sep_index]
        return float(timestamp_bytes.decode("utf-8"))
    except Exception:
        return None


async def send_time_message(client: Autobahn, payload: bytes, topic: str):
    await client.publish(topic, serialize_time_message(payload))


async def send_time_messages(
    client: Autobahn,
    extra_payload: bytes,
    topic: str,
    sleep: float | None = None,
    number_of_messages: int = 100,
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

    await client.subscribe(topic, callback)
    await asyncio.sleep(0.5)

    for _ in range(number_of_messages):
        await send_time_message(client, extra_payload, topic)
        if sleep is not None:
            await asyncio.sleep(sleep)

    while total_number_of_messages < number_of_messages:
        await asyncio.sleep(0.01)

    await client.unsubscribe(topic)

    return times
