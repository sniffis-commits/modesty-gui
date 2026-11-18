#!/usr/bin/env python3
"""Bridge: called by the Modesty desktop GUI; uses the nsfw_scanner API."""
import sys, json, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', '..', 'nsfw-lib', 'src'))

# Prefer bundled ffmpeg relative to this script, then installed path
script_dir = os.path.dirname(os.path.abspath(__file__))
ffmpeg_path = os.path.join(script_dir, 'ffmpeg', 'ffmpeg.exe')
ffprobe_path = os.path.join(script_dir, 'ffmpeg', 'ffprobe.exe')
if not os.path.isfile(ffmpeg_path):
    ffmpeg_path = r'C:\Program Files\1Tools\ffmpeg\bin\ffmpeg.exe'
if not os.path.isfile(ffprobe_path):
    ffprobe_path = r'C:\Program Files\1Tools\ffmpeg\bin\ffprobe.exe'
os.environ['NSFW_FFMPEG'] = ffmpeg_path
os.environ['NSFW_FFPROBE'] = ffprobe_path

from nsfw_scanner import set_model_mode
from nsfw_scanner.media import scan_video
from nsfw_scanner.skiplist import (
    DEFAULT_ACTIONS,
    parse_actions_string,
    write_result_skiplist,
)
from nsfw_scanner.webservice import WebserviceError, submit_media_file

def report_progress(info):
    print(json.dumps({'type': 'progress', 'data': info}), flush=True)


if __name__ == '__main__':
    path = sys.argv[1] if len(sys.argv) > 1 else ''
    path = os.path.abspath(path) if path else ''
    # Only fall back to workspace if path is a bare filename (no drive letter, no backslash start)
    if path and not os.path.isfile(path) and not (len(path) > 1 and path[1] == ':') and '\\' not in path.split('\\')[-1]:
        workspace_path = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), path)
        if os.path.isfile(workspace_path):
            path = workspace_path
    flags_json = sys.argv[2] if len(sys.argv) > 2 else '{}'
    try:
        flags = json.loads(flags_json)
    except Exception:
        flags = {}
    model = flags.get('model', 'auto')
    if model not in ('auto', 'joint', 'binary'):
        raise ValueError('model must be auto, joint, or binary')
    set_model_mode(model)
    result = scan_video(
        path,
        fps=flags.get('fps', 1.0),
        every_frame=flags.get('every_frame', False),
        max_frames=flags.get('max_frames', 2000),
        details=True,
        debug=flags.get('debug', False),
        subtitles=flags.get('subtitles', True),
        tiles=flags.get('tiles', 'off'),
        hwaccel=flags.get('hwaccel', 'none'),
        frames_format=flags.get('frames', 'raw'),
        backend=flags.get('decoder', 'auto'),
        face_crop=flags.get('face_crop', False),
        save_samples=True,
        include_frame_ratings=True,
        progress=report_progress,
    )
    skip_threshold = flags.get('skip_generation_threshold', 6)
    if (isinstance(skip_threshold, bool) or not isinstance(skip_threshold, int)
            or not 1 <= skip_threshold <= 10):
        raise ValueError('skip_generation_threshold must be an integer from 1 to 10')
    action_text = flags.get('skip_action_map', '').strip()
    actions_map = parse_actions_string(action_text) if action_text else DEFAULT_ACTIONS
    skiplist = write_result_skiplist(
        path,
        result,
        threshold=skip_threshold,
        actions_map=actions_map,
    )
    # Read preview text for GUI display
    preview_text = ""
    preview_path = skiplist.get('preview_path', '')
    if preview_path and os.path.isfile(preview_path):
        with open(preview_path, 'r', encoding='utf-8') as f:
            preview_text = f.read()
    # Only return serializable subset for the GUI
    output = {
        'rating': result.get('rating'),
        'frames': result.get('frame_count'),
        'hits': result.get('problem_count'),
        'scored_frames': result.get('scored_frames'),
        'romance_frames': result.get('romance_frames'),
        'worst_score': result.get('details', {}).get('worst_score'),
        'duration': result.get('duration_sec'),
        'synthetic': result.get('synthetic', False),
        'sample_count': len(result.get('saved_samples', [])),
        'skiplist_path': skiplist['path'],
        'skiplist_entries': skiplist['entry_count'],
        'skiplist_preview': preview_text,
    }
    if flags.get('submit_to_webservice'):
        try:
            output['webservice_submission'] = submit_media_file(
                flags.get('webservice_url', ''),
                path,
            )
        except WebserviceError as error:
            output['webservice_submission'] = {'error': str(error)}
    print(json.dumps({'type': 'result', 'data': output}), flush=True)
