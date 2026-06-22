# 🎵 Riffusion Kaggle Song Studio

**Remote-Controlled Long-Form AI Music Generation via Free Kaggle GPU**

This project transforms the open-source Riffusion model into a full-featured song generation service that runs on Kaggle's free T4 GPUs, accessible via REST API from your desktop or mobile app.

## ✨ Key Features

### 🎼 5-10 Minute Songs
- **Lyric Parsing**: Automatically detects section tags `[Verse]`, `[Chorus]`, `[Bridge]`, etc.
- **Energy Mapping**: Adjusts musical prompts based on section energy (chorus = high energy, verse = moderate)
- **Seamless Stitching**: Crossfades and normalizes audio for professional results

### 💾 Checkpointing
- Saves progress after each section
- Resumes from last completed section if notebook disconnects
- Essential for long-form generation on Kaggle's session-limited environment

### 🌐 Notebook-as-a-Service (NaaS)
- FastAPI server running inside Kaggle notebook
- Cloudflare tunnel provides public URL
- Full REST API for remote control

### 🆓 Completely Free
- Uses Kaggle's free GPU resources (T4 x2)
- No subscription fees
- No API costs

## 📁 Project Structure

```
kaggle_riffusion/
├── notebook/
│   ├── setup_and_serve.ipynb    # Master notebook to run on Kaggle
│   ├── src/
│   │   ├── __init__.py
│   │   ├── api_server.py        # FastAPI endpoints
│   │   ├── job_queue.py         # Async job management
│   │   ├── model_manager.py     # Riffusion pipeline & VRAM
│   │   ├── song_arranger.py     # Lyric parsing & structure
│   │   ├── long_form_generator.py  # Sequential clip generation
│   │   ├── audio_stitcher.py    # FFmpeg/pydub crossfading
│   │   ├── preset_engine.py     # Musical style definitions
│   │   ├── tunnel_manager.py    # Cloudflare tunnel
│   │   └── utils.py             # Utilities
│   ├── presets/
│   │   └── styles.yaml          # Genre/style presets
│   └── requirements.txt
├── client/
│   ├── kaggle_client.py         # Python SDK
│   └── example_bulk_usage.py    # Usage examples
└── config/
    └── kaggle_instances.yaml    # Configuration
```

## 🚀 Quick Start

### Step 1: Upload to Kaggle

1. Create a new Kaggle notebook
2. Upload all files from `notebook/` directory
3. Enable GPU acceleration (Settings → Accelerator → GPU T4 x2)
4. Enable Internet access (Settings → Internet → On)

### Step 2: Run the Notebook

Execute all cells in `setup_and_serve.ipynb` in order:

1. **Install Dependencies** - Installs torch, diffusers, pydub, ffmpeg, etc.
2. **Install Cloudflared** - Sets up the tunnel software
3. **Setup Source Package** - Configures Python imports
4. **Check GPU** - Verifies GPU availability
5. **Start Server** - Launches FastAPI + Cloudflare tunnel

### Step 3: Get Your Tunnel URL

After running, you'll see output like:

```
============================================================
🎉 SERVER READY!
============================================================

📡 Public API URL: https://a1b2c3d4-8000.us.cloudflarestage.com

💡 Usage:
   GET  https://.../health - Health check
   GET  https://.../presets - List presets
   POST https://.../generate_song - Generate a song
   GET  https://.../status/{job_id} - Check status
   GET  https://.../download/{job_id} - Download result

============================================================
⏳ Keep this notebook running to maintain the server
============================================================
```

### Step 4: Use the Client SDK

```python
from kaggle_client import RiffusionSongClient

# Initialize with your tunnel URL
client = RiffusionSongClient("https://your-tunnel.trycloudflare.com")

# Submit a song
job_id = client.submit_song(
    title="My AI Song",
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

# Wait for completion
status = client.poll_until_complete(job_id, callback=lambda s: print(s['status']))

# Download result
if status['status'] == 'completed':
    client.download_song(job_id, "./output/my_song.wav")
```

## 🎹 Available Style Presets

| Preset | Description |
|--------|-------------|
| `epic_orchestral` | Cinematic, Hans Zimmer-style orchestral |
| `synthwave_retro` | 80s analog synths, driving bassline |
| `lofi_hiphop` | Chill beats, vinyl crackle |
| `ambient_electronic` | Atmospheric pads, ethereal textures |
| `rock_alternative` | Electric guitars, dynamic drums |
| `jazz_smooth` | Saxophone, upright bass, brushed drums |
| `cinematic_trailer` | Epic hits, rising tension |
| `pop_upbeat` | Catchy melodies, radio-friendly |

## 📡 API Reference

### Core Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/presets` | List available presets |
| `POST` | `/generate_song` | Submit new song |
| `POST` | `/generate_alternates` | Create variations |
| `POST` | `/generate_bulk` | Multiple songs |
| `GET` | `/status/{job_id}` | Job status |
| `GET` | `/download/{job_id}` | Download final song |
| `GET` | `/download_alternates/{job_id}` | Download ZIP |
| `POST` | `/cancel/{job_id}` | Cancel job |

### Example: Generate Song Request

```json
POST /generate_song
{
  "title": "Digital Dreams",
  "lyrics": "[Verse]\nIn the code we trust\n[Chorus]\nDigital dreams arise",
  "style_preset": "synthwave_retro",
  "target_duration_minutes": 5.0,
  "seed": 42
}
```

Response:
```json
{
  "job_id": "uuid-here",
  "status": "queued"
}
```

## 🔧 How It Solves the 5-Second Problem

Riffusion natively generates ~5-second clips. Here's how we create 5-10 minute songs:

### Architecture Flow

```
Raw Lyrics
    ↓
[Song Arranger]
    ├─ Parse section tags ([Verse], [Chorus], etc.)
    ├─ Calculate duration per section
    ├─ Synthesize prompts with energy modifiers
    └─ Determine clip count per section
    ↓
[Long Form Generator]
    ├─ For each section:
    │   ├─ Generate N x 5-second clips
    │   ├─ Clear VRAM between sections
    │   └─ Save checkpoint
    ↓
[Audio Stitcher]
    ├─ Apply crossfades (1-1.5s)
    ├─ Normalize loudness (-16 LUFS)
    └─ Final fade-out
    ↓
Complete 5-10 Minute Song
```

### Section Energy Mapping

| Section | Energy | Prompt Modifier | Duration Ratio |
|---------|--------|-----------------|----------------|
| Intro | 0.6 | "building up, introductory" | 10% |
| Verse | 0.7 | "steady rhythm, focused" | 25% |
| Chorus | 0.95 | "high energy, anthemic" | 25% |
| Bridge | 0.8 | "shifting tone, building" | 15% |
| Outro | 0.5 | "fading out, resolving" | 10% |

## ⚠️ Important Notes

### Kaggle Limitations
- **Session Timeout**: Notebooks run for max 12 hours
- **GPU Availability**: Subject to Kaggle's GPU quota
- **Internet Required**: Must enable "Internet" in notebook settings
- **Storage**: ~20GB available in `/kaggle/working`

### Best Practices
1. **Keep Tab Open**: Kaggle may disconnect idle notebooks
2. **Download Promptly**: Files are deleted when session ends
3. **Use Checkpointing**: Long songs benefit from section checkpoints
4. **Test Short First**: Start with 5-minute songs before attempting 10-minute epics

## 🛠️ Development

### Local Testing (without Kaggle)

```bash
cd notebook/src

# Install dependencies
pip install -r ../requirements.txt

# Run server locally
python -m api_server
```

### Adding New Presets

Edit `presets/styles.yaml`:

```yaml
presets:
  your_new_style:
    base_prompt: "your description here"
    negative_prompt: "what to avoid"
    tempo_bpm: 120
    key: "C major"
    instrumentation: "instruments"
```

## 📄 License

This project integrates the open-source Riffusion model. Please review Riffusion's license terms at https://github.com/riffusion/riffusion

## 🙏 Credits

- **Riffusion**: https://github.com/riffusion/riffusion
- **Diffusers**: https://github.com/huggingface/diffusers
- **Kaggle**: Free GPU platform https://www.kaggle.com

---

Built for the Lightkid AI Studio project.
