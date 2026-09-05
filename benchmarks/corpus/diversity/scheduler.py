def cache_refresh_task():
    """Schedule a refresh when a cached entry expires."""
    return enqueue_refresh()
