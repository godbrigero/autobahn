import time


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
        # If the payload cannot be read or decoding fails, return None to indicate failure
        return None
