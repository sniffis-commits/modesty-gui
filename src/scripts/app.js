import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { confirm, open, save } from '@tauri-apps/plugin-dialog';

const DEFAULT_FLAGS = {
  fps: 1,
  every_frame: false,
  max_frames: 2000,
  subtitles: true,
  tiles: 'off',
  hwaccel: 'none',
  frames: 'raw',
  decoder: 'auto',
  model: 'auto',
  face_crop: false,
  debug: false,
  skip_generation_threshold: 6,
  skip_action_map: '',
  submit_to_webservice: false,
  webservice_url: ''
};

const App = {
  state: {
    project: null,
    samples: [],
    flags: { ...DEFAULT_FLAGS },
    mediaPath: '',
    sampleMediaPath: '',
    modalOriginalRevealed: false,
    logLines: []
  },

  init() {
    this.setupTabs();
    this.syncControlsFromFlags();
    this.setupFileSelects();
    this.setupScanControls();
    this.setupSampleGrid();
    this.setupModal();
    this.setupProjectButtons();
    this.addLog('Ready');
  },

  // ----- Tab System -----
  setupTabs() {
    document.querySelectorAll('.tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
        document.querySelectorAll('.tab-panel').forEach(p => p.classList.add('hidden'));
        btn.classList.add('active');
        document.getElementById(`tab-${btn.dataset.tab}`).classList.remove('hidden');
      });
    });
  },

  syncControlsFromFlags() {
    const controls = {
      fps: ['fps', 'value'],
      every_frame: ['every-frame', 'checked'],
      max_frames: ['max-frames', 'value'],
      subtitles: ['subtitles', 'checked'],
      tiles: ['tiles', 'value'],
      hwaccel: ['hwaccel', 'value'],
      frames: ['frames-format', 'value'],
      decoder: ['decoder', 'value'],
      model: ['model', 'value'],
      face_crop: ['face-crop', 'checked'],
      debug: ['debug-scan', 'checked'],
      skip_generation_threshold: ['generated-skip-threshold', 'value'],
      skip_action_map: ['skip-action-map', 'value'],
      submit_to_webservice: ['submit-to-webservice', 'checked'],
      webservice_url: ['webservice-url', 'value']
    };
    for (const [flag, [id, property]] of Object.entries(controls)) {
      const element = document.getElementById(id);
      if (element) element[property] = this.state.flags[flag] ?? DEFAULT_FLAGS[flag];
    }
    this.readFlagsFromControls();
  },

  readFlagsFromControls() {
    this.state.flags = {
      ...this.state.flags,
      fps: Number(document.getElementById('fps').value),
      every_frame: document.getElementById('every-frame').checked,
      max_frames: Number(document.getElementById('max-frames').value),
      subtitles: document.getElementById('subtitles').checked,
      tiles: document.getElementById('tiles').value,
      hwaccel: document.getElementById('hwaccel').value,
      frames: document.getElementById('frames-format').value,
      decoder: document.getElementById('decoder').value,
      model: document.getElementById('model').value,
      face_crop: document.getElementById('face-crop').checked,
      debug: document.getElementById('debug-scan').checked,
      skip_generation_threshold: Number(document.getElementById('generated-skip-threshold').value),
      skip_action_map: document.getElementById('skip-action-map').value.trim(),
      submit_to_webservice: document.getElementById('submit-to-webservice').checked,
      webservice_url: document.getElementById('webservice-url').value.trim()
    };
  },

  // ----- Scan Buttons -----
  setupScanControls() {
    document.getElementById('btn-start-scan').addEventListener('click', async () => {
      this.setStatus('Starting scan...');
      const startButton = document.getElementById('btn-start-scan');
      let unlistenProgress;
      startButton.disabled = true;
      try {
        this.readFlagsFromControls();
        if (!Number.isInteger(this.state.flags.skip_generation_threshold) ||
            this.state.flags.skip_generation_threshold < 1 ||
            this.state.flags.skip_generation_threshold > 10) {
          this.addLog('Skip-list generation threshold must be an integer from 1 to 10');
          this.setStatus('Scan failed: invalid skip-list threshold', true);
          return;
        }
        const path = document.getElementById('media-file-path').value.trim().replace(/^"(.*)"$/, '$1');
        if (!path) {
          this.addLog('Select a media file or enter its full path');
          this.setStatus('Scan failed: no media file selected', true);
          return;
        }
        if (!Number.isFinite(this.state.flags.fps) ||
            this.state.flags.fps < 0.1 || this.state.flags.fps > 30) {
          this.addLog('Frames per second must be between 0.1 and 30');
          this.setStatus('Scan failed: invalid FPS', true);
          return;
        }
        if (!Number.isInteger(this.state.flags.max_frames) ||
            this.state.flags.max_frames < 1 || this.state.flags.max_frames > 100000) {
          this.addLog('Maximum frames must be a whole number from 1 to 100000');
          this.setStatus('Scan failed: invalid maximum frame count', true);
          return;
        }
        if (this.state.flags.submit_to_webservice && !this.state.flags.webservice_url) {
          this.addLog('Enter a webservice base URL or turn off hosted submission');
          this.setStatus('Scan failed: missing webservice URL', true);
          return;
        }
        if (this.state.flags.submit_to_webservice) {
          let serviceUrl;
          try {
            serviceUrl = new URL(this.state.flags.webservice_url);
          } catch {
            this.addLog('Enter a valid HTTP(S) webservice base URL');
            this.setStatus('Scan failed: invalid webservice URL', true);
            return;
          }
          const localHosts = ['localhost', '127.0.0.1', '[::1]'];
          if ((serviceUrl.protocol !== 'https:' &&
               !(serviceUrl.protocol === 'http:' && localHosts.includes(serviceUrl.hostname))) ||
              serviceUrl.username || serviceUrl.password || serviceUrl.search || serviceUrl.hash) {
            this.addLog('Use HTTPS for hosted uploads, or plain HTTP only for localhost');
            this.setStatus('Scan failed: insecure webservice URL', true);
            return;
          }
        }
        const scanPath = path;
        // Show progress card
        document.getElementById('progress-card').classList.remove('hidden');
        document.getElementById('progress-bar').style.width = '0%';
        document.getElementById('progress-status').textContent = 'Scanning... The window remains responsive.';
        document.getElementById('progress-stats').textContent = 'frames: 0 | hits: 0 | ratings: -';
        this.state.sampleMediaPath = scanPath;
        this.state.samples = [];
        this.renderSamples(this.state.samples);
        const projectName = scanPath.split(/[\\/]/).pop().replace(/\.[^.]+$/, '');
        this.state.project = {
          project_id: projectName,
          original_filename: scanPath.split(/[\\/]/).pop(),
          sha256: '',
          timestamp: Date.now(),
          flags: this.state.flags,
          samples: [],
          timeline: [],
          generated_skiplist_path: null,
          generated_skiplist_entries: 0,
          generated_skiplist_preview: null
        };
        this.updateGeneratedSkiplist();

        unlistenProgress = await listen('scan-progress', ({ payload }) => {
          const stageLabels = {
            probe: 'Checking video',
            frames: 'Scanning frames',
            subtitles: 'Scanning subtitles',
            done: 'Finishing scan'
          };
          const stage = stageLabels[payload.stage] ?? payload.stage ?? 'Scanning';
          const percent = Number.isFinite(payload.percent) ? payload.percent : null;
          if (percent !== null) {
            document.getElementById('progress-bar').style.width = `${percent}%`;
          }
          const frameProgress = payload.total
            ? `${payload.done} / ${payload.total} frames`
            : `${payload.done ?? 0} frames`;
          const timecode = payload.timecode ? ` | time: ${payload.timecode}` : '';
          const eta = Number.isFinite(payload.eta) ? ` | ETA: ${payload.eta}s` : '';
          document.getElementById('progress-status').textContent =
            `${stage}${percent === null ? '' : ` (${percent}%)`}${timecode}${eta}`;
          document.getElementById('progress-stats').textContent =
            `${frameProgress} | hits: ${payload.problems ?? 0}`;
          if (payload.sample) {
            this.upsertSample(payload.sample);
          }
        });

        const result = await invoke('scan_video_cmd', {
          path: scanPath,
          flagsJson: JSON.stringify(this.state.flags)
        });
        const data = JSON.parse(result);
        document.getElementById('progress-bar').style.width = '100%';
        document.getElementById('progress-status').textContent = 'Scanned ' + (data.frames ?? 0) + ' frames, ' + (data.hits ?? 0) + ' hits';
        document.getElementById('progress-stats').textContent =
          'frames: ' + (data.frames ?? 0) + ' | hits: ' + (data.hits ?? 0) +
          ' | saved frames: ' + (data.sample_count ?? 0) + ' | rating: ' + (data.rating ?? '-');
        this.state.project.generated_skiplist_path = data.skiplist_path;
        this.state.project.generated_skiplist_entries = data.skiplist_entries ?? 0;
        this.state.project.generated_skiplist_preview = data.skiplist_preview ?? null;
        this.updateGeneratedSkiplist();
        this.addLog(`Video scan: ${data.rating ?? '-'} (${data.frames ?? 0} frames, ${data.hits ?? 0} hits)`);
        this.addLog(`Generated skip list saved: ${data.skiplist_path} (${data.skiplist_entries ?? 0} entries)`);
        if (data.webservice_submission?.error) {
          this.addLog(`Hosted scan submission failed: ${data.webservice_submission.error}`);
          this.setStatus('Local scan complete; hosted submission failed', true);
        } else if (data.webservice_submission) {
          this.addLog(`Hosted scan rating: ${data.webservice_submission.rating}`);
          this.setStatus(`Local scan complete (${data.rating ?? 'unknown'}); hosted scan ${data.webservice_submission.rating}`);
        } else {
          this.setStatus('Scan complete (rating ' + (data.rating ?? 'unknown') + ')');
        }
        if (this.state.samples.length > 0) {
          document.querySelector('[data-tab="samples"]').click();
        }
      } catch (err) {
        this.addLog(`Error: ${err}`);
        this.setStatus('Scan failed', true);
      } finally {
        unlistenProgress?.();
        startButton.disabled = false;
      }
    });
    document.getElementById('btn-stop-scan').addEventListener('click', () => {
      this.setStatus('Stopping scan...');
      this.clearLog('Scan stopped');
      document.getElementById('progress-card').classList.add('hidden');
    });
  },

  // ----- File Select Wiring -----
  setupFileSelects() {
    document.getElementById('btn-export-generated-skiplist').addEventListener('click', async () => {
      const sourcePath = this.state.project?.generated_skiplist_path;
      if (!sourcePath) {
        this.addLog('No generated skip list to export');
        return;
      }
      try {
        const outputPath = await save({
          defaultPath: sourcePath.replace(/\.generated\.skp$/i, '.skp'),
          filters: [{ name: 'Skip Lists', extensions: ['skp'] }]
        });
        if (!outputPath) return;
        const exportedPath = await invoke('export_skiplist_cmd', {
          sourcePath,
          outputPath
        });
        this.addLog(`Exported skip list: ${exportedPath}`);
        this.setStatus('Skip list exported');
      } catch (error) {
        this.addLog(`Skip list export failed: ${error}`);
        this.setStatus('Skip list export failed', true);
      }
    });
    document.getElementById('btn-browse-media').addEventListener('click', async () => {
      try {
        const path = await invoke('open_dialog_select');
        if (path) {
          document.getElementById('media-file-path').value = path;
          this.state.mediaPath = path;
        }
      } catch (e) {
        this.addLog(`Media file selection failed: ${e}`);
      }
    });
    document.getElementById('media-file-path').addEventListener('input', e => {
      this.state.mediaPath = e.target.value.trim().replace(/^"(.*)"$/, '$1');
    });
    document.getElementById('btn-browse-dev-image').addEventListener('click', () => document.getElementById('file-input-dev-image').click());
    document.getElementById('file-input-dev-image').addEventListener('change', (e) => {
      const f = e.target.files[0];
      if (f) { document.getElementById('dev-image-path').value = f.path || f.name; }
    });
    document.getElementById('btn-scan-dev-image').addEventListener('click', async () => {
      const path = document.getElementById('dev-image-path').value;
      if (!path) { this.addLog('No image selected'); return; }
      try {
        const flagsStr = JSON.stringify(this.state.flags);
        const result = await invoke('scan_image_cmd', { path, flagsJson: flagsStr });
        document.getElementById('dev-image-result').textContent = JSON.stringify(result, null, 2);
        this.addLog(`Image scan: rating=${result.rating ?? '?'}`);
      } catch (e) {
        this.addLog(`Image scan error: ${e}`);
      }
    });
  },

  // ----- Sample Grid -----
  async setupSampleGrid() {
    document.getElementById('btn-reload-samples').addEventListener('click', async () => {
      if (!this.state.sampleMediaPath) {
        this.addLog('Open or scan a project first to view its saved frames');
        return;
      }
      try {
        const list = await invoke('list_project_samples_cmd', {
          samplePaths: this.state.project?.samples ?? []
        });
        this.state.samples = list;
        this.renderSamples(list);
        this.addLog(`Loaded ${list.length} saved frames`);
      } catch (e) {
        this.addLog(`Failed to load samples: ${e}`);
      }
    });
  },

  renderSamples(samples) {
    const grid = document.getElementById('sample-grid');
    grid.innerHTML = '';
    if (samples.length === 0) {
      grid.innerHTML = '<div class="sample-empty">No detected frames were saved.</div>';
    }
    samples.forEach(s => {
      const item = document.createElement('button');
      item.className = 'sample-item';
      item.type = 'button';
      const name = document.createElement('span');
      name.className = 'sample-name';
      name.textContent = s.filename;
      const meta = document.createElement('span');
      meta.className = 'meta';
      meta.textContent = `Rating: ${s.rating ?? 'N/A'} | blurred preview`;
      item.append(name, meta);
      item.addEventListener('click', () => this.showModal(s));
      grid.appendChild(item);
    });
    document.getElementById('sample-count').textContent = `${samples.length} samples`;
  },

  upsertSample(sample) {
    const index = this.state.samples.findIndex(item => item.filename === sample.filename);
    if (index >= 0) {
      this.state.samples[index] = sample;
    } else {
      this.state.samples.push(sample);
    }
    this.state.samples.sort((left, right) => left.filename.localeCompare(right.filename));
    if (this.state.project) {
      this.state.project.samples = this.state.samples.map(item => item.original_path);
    }
    this.renderSamples(this.state.samples);
  },

  // ----- Modal Handling -----
  setupModal() {
    document.querySelectorAll('.modal-close-btn').forEach(btn => {
      btn.addEventListener('click', e => { e.preventDefault(); e.target.closest('.modal').classList.add('hidden'); });
    });
    document.querySelector('.modal-close').addEventListener('click', e => {
      e.preventDefault(); e.target.closest('.modal').classList.add('hidden');
    });
    document.querySelector('.modal-backdrop').addEventListener('click', e => {
      if (e.target === e.currentTarget) document.getElementById('sample-modal').classList.add('hidden');
    });
    document.getElementById('modal-show-original').addEventListener('click', async () => {
      const sample = this.state.modalSample;
      if (!sample) return;
      if (this.state.modalOriginalRevealed) {
        if (await this.loadSamplePreview(sample.blurred_path)) {
          this.state.modalOriginalRevealed = false;
          document.getElementById('modal-info').textContent =
            `Rating: ${sample.rating ?? 'N/A'} | Preview: blurred`;
          document.getElementById('modal-show-original').textContent = 'Show Original (Explicit Warning)';
        }
        return;
      }
      const confirmed = await confirm(
        'The unblurred frame may contain sexually explicit or otherwise sensitive content. Show it?',
        { title: 'Sensitive content warning', kind: 'warning' }
      );
      if (!confirmed) return;
      if (await this.loadSamplePreview(sample.original_path)) {
        this.state.modalOriginalRevealed = true;
        document.getElementById('modal-info').textContent =
          `Rating: ${sample.rating ?? 'N/A'} | Unblurred original shown`;
        document.getElementById('modal-show-original').textContent = 'Hide Original';
      }
    });
    document.getElementById('modal-rescan-frame').addEventListener('click', async event => {
      const sample = this.state.modalSample;
      if (!sample) return;
      const button = event.currentTarget;
      button.disabled = true;
      try {
        const result = await invoke('rescan_sample_cmd', {
          path: sample.original_path,
          flagsJson: JSON.stringify(this.state.flags)
        });
        sample.rating = result.rating ?? null;
        this.upsertSample(sample);
        if (this.state.modalSample === sample) {
          document.getElementById('modal-info').textContent =
            `Rating: ${sample.rating ?? 'N/A'} | Preview: blurred`;
        }
        this.addLog(`Rescanned ${sample.filename}: rating=${sample.rating ?? 'N/A'}`);
      } catch (e) {
        this.addLog(`Frame rescan failed: ${e}`);
        this.setStatus('Frame rescan failed', true);
      } finally {
        button.disabled = false;
      }
    });
  },

  async showModal(sample) {
    this.state.modalSample = sample;
    this.state.modalOriginalRevealed = false;
    document.getElementById('modal-title').textContent = sample.filename;
    document.getElementById('modal-info').textContent =
      `Rating: ${sample.rating ?? 'N/A'} | Preview: blurred`;
    document.getElementById('modal-img').src = '';
    document.getElementById('modal-show-original').classList.remove('hidden');
    document.getElementById('modal-show-original').textContent = 'Show Original (Explicit Warning)';
    document.getElementById('sample-modal').classList.remove('hidden');
    await this.loadSamplePreview(sample.blurred_path);
  },

  async loadSamplePreview(path) {
    try {
      const sample = this.state.modalSample;
      const image = await invoke('read_sample_cmd', { path });
      if (this.state.modalSample !== sample) {
        return false;
      }
      document.getElementById('modal-img').src = image;
      return true;
    } catch (e) {
      this.addLog(`Failed to load frame preview: ${e}`);
      this.setStatus('Frame preview failed to load', true);
      return false;
    }
  },

  // ----- Log -----
  addLog(msg) {
    const line = `${new Date().toLocaleTimeString()} ${msg}`;
    this.state.logLines.push(line);
    if (this.state.logLines.length > 200) this.state.logLines.shift();
    const out = document.getElementById('log-output');
    out.textContent += '\n' + line;
    out.scrollTop = out.scrollHeight;
  },

  clearLog(msg) {
    document.getElementById('log-output').textContent = msg ? `${new Date().toLocaleTimeString()} ${msg}` : '';
    this.state.logLines = [];
  },

  // ----- Status Bar -----
  setStatus(text, isError) {
    const el = document.getElementById('status-ready');
    el.textContent = text;
    el.style.color = isError ? 'var(--red)' : '#aaa';
  },

  // ----- Project Persistence -----
  setupProjectButtons() {
    document.getElementById('btn-new').addEventListener('click', () => {
      this.state.project = null;
      this.state.samples = [];
      this.state.sampleMediaPath = '';
      this.updateGeneratedSkiplist();
      this.renderSamples(this.state.samples);
      this.addLog('New project started');
    });
    document.getElementById('btn-open').addEventListener('click', async () => {
      try {
        const path = await open({
          properties: {
            title: 'Open Project JSON',
            filters: [{ name: 'JSON Files', extensions: ['json'] }]
          }
        });
        if (path) {
          this.state.project = await invoke('import_project_cmd', { jsonPath: path });
          this.state.flags = Object.fromEntries(
            Object.keys(DEFAULT_FLAGS).map(key => [
              key,
              this.state.project.flags?.[key] ?? DEFAULT_FLAGS[key]
            ])
          );
          this.syncControlsFromFlags();
          this.state.sampleMediaPath = this.state.project.original_filename;
          this.state.samples = await invoke('list_project_samples_cmd', {
            samplePaths: this.state.project.samples
          });
          this.state.flags.skip_generation_threshold ??= 6;
          document.getElementById('generated-skip-threshold').value =
            this.state.flags.skip_generation_threshold;
          this.updateGeneratedSkiplist();
          this.renderSamples(this.state.samples);
          this.addLog(`Opened project: ${this.state.project.project_id}`);
        }
      } catch (e) {
        this.addLog(`Open error: ${e}`);
      }
    });
    document.getElementById('btn-save').addEventListener('click', async () => {
      try {
        this.readFlagsFromControls();
        this.state.project = {
          ...this.state.project,
          timestamp: Date.now(),
          flags: this.state.flags
        };
        const path = await save({
          properties: {
            title: 'Save Project JSON',
            defaultPath: `./${this.state.project?.project_id || 'project'}_export.json`,
            filters: [{ name: 'JSON Files', extensions: ['json'] }]
          }
        });
        if (path) {
          await invoke('export_project_cmd', {
            projectJson: JSON.stringify(this.state.project),
            outputPath: path
          });
          this.addLog('Project saved successfully');
        }
      } catch (e) {
        this.addLog(`Save error: ${e}`);
      }
    });
  },

  updateGeneratedSkiplist() {
    const project = this.state.project;
    const status = document.getElementById('generated-skiplist-status');
    const exportButton = document.getElementById('btn-export-generated-skiplist');
    const previewEl = document.getElementById('generated-skiplist-preview');
    if (!project?.generated_skiplist_path) {
      status.textContent = 'A skip list is saved automatically with each video scan, even if you do not export it.';
      exportButton.disabled = true;
      if (previewEl) previewEl.textContent = '';
      return;
    }
    status.textContent = `${project.generated_skiplist_entries ?? 0} entries saved to ${project.generated_skiplist_path}`;
    exportButton.disabled = false;
    if (previewEl && project.generated_skiplist_preview) {
      previewEl.textContent = project.generated_skiplist_preview;
      previewEl.style.display = 'block';
    } else if (previewEl) {
      previewEl.style.display = 'none';
    }
  }
};

App.init();
c