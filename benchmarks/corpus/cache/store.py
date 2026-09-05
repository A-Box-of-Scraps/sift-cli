def evict_least_recently_used(entries):
    """Remove the oldest cache entry when capacity is exceeded."""
    return entries.pop(0)
