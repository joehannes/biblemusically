"""
Example usage of the Riffusion Kaggle Client SDK

Demonstrates bulk operations and alternate generation.
"""

from kaggle_client import RiffusionSongClient, SongConfig


def example_single_song():
    """Generate a single song."""
    # Initialize client with your tunnel URL
    client = RiffusionSongClient("https://your-tunnel.trycloudflare.com")
    
    # Submit a song
    job_id = client.submit_song(
        title="Digital Dreams",
        lyrics="""
        [Verse 1]
        In the digital dreams we weave
        Through the code we believe
        
        [Chorus]
        Singing loud for all to hear
        The future of music is here
        """,
        style="synthwave_retro",
        duration_mins=5.0
    )
    
    print(f"Job submitted: {job_id}")
    
    # Poll until complete with progress callback
    def on_progress(status):
        print(f"Progress: {status['status']} - {status.get('progress', {})}")
    
    final_status = client.poll_until_complete(job_id, callback=on_progress)
    
    # Download result
    if final_status["status"] == "completed":
        output_path = client.download_song(job_id, "./output/digital_dreams.wav")
        print(f"Song saved to: {output_path}")


def example_bulk_generation():
    """Generate multiple songs in bulk."""
    client = RiffusionSongClient("https://your-tunnel.trycloudflare.com")
    
    # Define multiple songs
    songs = [
        {
            "title": "Lofi Study Session",
            "lyrics": "[Verse]\nQuiet night, books open wide\n[Chorus]\nLearning flows like a gentle tide",
            "style": "lofi_hiphop",
            "duration_minutes": 5.0
        },
        {
            "title": "Epic Journey",
            "lyrics": "[Intro]\nThe adventure begins\n[Verse]\nMountains high and valleys deep\n[Chorus]\nOnward we shall sweep",
            "style": "epic_orchestral",
            "duration_minutes": 6.0
        },
        {
            "title": "Midnight Drive",
            "lyrics": "[Verse]\nNeon lights blur past\n[Chorus]\nSpeeding free at last",
            "style": "synthwave_retro",
            "duration_minutes": 5.5
        }
    ]
    
    # Submit all songs
    job_ids = client.submit_bulk_songs(songs)
    print(f"Bulk submitted {len(job_ids)} jobs: {job_ids}")
    
    # Monitor all jobs
    def on_job_progress(job_id, status):
        print(f"[{job_id[:8]}...] {status['status']}")
    
    results = client.poll_bulk_jobs(job_ids, callback=on_job_progress)
    
    # Download completed songs
    for job_id, status in results.items():
        if status["status"] == "completed":
            title = status.get("title", "song")
            safe_title = "".join(c for c in title if c.isalnum() or c in " -_")
            client.download_song(job_id, f"./output/{safe_title}.wav")


def example_alternate_versions():
    """Generate alternate versions of a song."""
    client = RiffusionSongClient("https://your-tunnel.trycloudflare.com")
    
    # First, generate an original song
    original_job_id = client.submit_song(
        title="Summer Vibes",
        lyrics="[Verse]\nSunshine warming up the day\n[Chorus]\nSummer vibes are here to stay",
        style="pop_upbeat",
        duration_mins=5.0
    )
    
    # Wait for completion
    status = client.poll_until_complete(original_job_id)
    
    if status["status"] == "completed":
        # Generate 3 alternate versions with different seeds
        alt_job_ids = client.request_alternates(
            original_job_id,
            count=3,
            variation_type="seed"
        )
        
        print(f"Generated {len(alt_job_ids)} alternates: {alt_job_ids}")
        
        # Wait for alternates to complete
        results = client.poll_bulk_jobs(alt_job_ids)
        
        # Download as ZIP
        client.download_alternates_zip(
            original_job_id,
            "./output/summer_vibes_alternates.zip"
        )


def example_with_custom_seed():
    """Generate reproducible songs using seeds."""
    client = RiffusionSongClient("https://your-tunnel.trycloudflare.com")
    
    # Use a specific seed for reproducibility
    job_id = client.submit_song(
        title="Reproducible Track",
        lyrics="[Verse]\nSame seed, same result\n[Chorus]\nDeterministic melody",
        style="ambient_electronic",
        duration_mins=5.0,
        seed=12345  # Fixed seed
    )
    
    # Later, use the same seed to get similar results
    job_id_2 = client.submit_song(
        title="Reproducible Track v2",
        lyrics="[Verse]\nSame seed, same result\n[Chorus]\nDeterministic melody",
        style="ambient_electronic",
        duration_mins=5.0,
        seed=12345  # Same seed should produce similar output
    )


if __name__ == "__main__":
    import os
    
    # Check for environment variable
    tunnel_url = os.environ.get("RIFFUSION_TUNNEL_URL")
    
    if not tunnel_url:
        print("Set RIFFUSION_TUNNEL_URL environment variable to run examples")
        print("\nExample:")
        print("  export RIFFUSION_TUNNEL_URL=https://your-tunnel.trycloudflare.com")
        print("\nOr modify the script to use your tunnel URL directly.")
    else:
        print("Running single song example...")
        example_single_song()
