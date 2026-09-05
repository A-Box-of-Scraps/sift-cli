def retry_metrics(attempt):
    """Record retry attempts, without scheduling requests."""
    return {"retry_attempt": attempt}
