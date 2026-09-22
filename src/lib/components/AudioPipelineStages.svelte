<script lang="ts">
  import type { AudioPipelineInfo } from '$lib/types';
  import { i18n } from '$lib/stores/i18n.svelte';
  import {
    CpuIcon as Cpu,
    SlidersIcon as Sliders,
    SpeakerHighIcon as Speaker,
    SparkleIcon as Sparkles,
  } from 'phosphor-svelte';

  interface Props {
    pipeline: AudioPipelineInfo | null;
    class?: string;
  }

  let { pipeline, class: className = '' }: Props = $props();

  function formatChannels(channels?: number): string {
    if (!channels) return 'Stereo (2 ch)';
    if (channels === 1) return 'Mono (1 ch)';
    if (channels === 2) return 'Stereo (2 ch)';
    return `${channels} ch`;
  }

  function formatSampleRate(rate?: number): string {
    if (!rate) return '—';
    return `${(rate / 1000).toFixed(rate % 1000 === 0 ? 0 : 1)} kHz`;
  }

  function getQualityTierLabel(tier: string): string {
    switch (tier) {
      case 'hi-res':
        return i18n.t('audioPipeline.tierHiRes', {}, 'Hi-Res Audio');
      case 'hq':
        return i18n.t('audioPipeline.tierHq', {}, 'High Quality');
      case 'sq':
        return i18n.t('audioPipeline.tierSq', {}, 'Standard Quality');
      case 'lq':
      default:
        return i18n.t('audioPipeline.tierLq', {}, 'Low Quality');
    }
  }

  let isBitPerfect = $derived.by(() => {
    if (!pipeline) return false;
    const noEq = !pipeline.eq_enabled;
    const noLoudness = pipeline.loudness_source === 'disabled';
    const noResample = !pipeline.resample_rate || pipeline.resample_rate === pipeline.output_sample_rate;
    return noEq && noLoudness && noResample;
  });
</script>

<div class={`space-y-4 select-text ${className}`}>
  {#if !pipeline}
    <div class="text-xs text-brand-text-primary py-4 text-center">
      {i18n.t('playerBar.noSongPlaying', {}, 'No song currently playing')}
    </div>
  {:else}
    <!-- Stage 1: Input -->
    <div>
      <div class="flex items-center justify-between gap-2 mb-2">
        <div class="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-brand-text-primary">
          <Cpu class="w-3.5 h-3.5 text-brand-text-primary" />
          <span>{i18n.t('audioPipeline.stageInput', {}, 'Input')}</span>
        </div>
        <span class="px-2 py-0.5 text-[10px] font-semibold tracking-wider bg-brand-accent/15 text-brand-text-primary border border-brand-accent/30 rounded-full">
          {getQualityTierLabel(pipeline.quality_tier)}
        </span>
      </div>

      <div class="grid grid-cols-2 gap-x-3 gap-y-1.5 text-xs bg-brand-bg/40 p-2.5 rounded-lg border border-brand-border/30">
        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.codec', {}, 'Codec')}</span>
          <span class="text-brand-text-primary font-semibold uppercase">{pipeline.input_format}</span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.bitrate', {}, 'Bitrate')}</span>
          <span class="text-brand-text-primary font-semibold">
            {pipeline.input_bitrate_kbps ? `${pipeline.input_bitrate_kbps} kbps` : '—'}
          </span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.sampleRate', {}, 'Sample Rate')}</span>
          <span class="text-brand-text-primary font-semibold">{formatSampleRate(pipeline.input_sample_rate)}</span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.bitDepth', {}, 'Bit Depth')}</span>
          <span class="text-brand-text-primary font-semibold">{pipeline.input_bit_depth ? `${pipeline.input_bit_depth}-bit` : '16-bit / lossy'}</span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.channels', {}, 'Channels')}</span>
          <span class="text-brand-text-primary font-semibold">{formatChannels(pipeline.input_channels)}</span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.decoder', {}, 'Decoder')}</span>
          <span class="text-brand-text-primary font-semibold truncate" title={pipeline.decoder_name}>{pipeline.decoder_name}</span>
        </div>
      </div>
    </div>

    <!-- Stage 2: Processing (DSP & Effects) -->
    <div>
      <div class="flex items-center justify-between gap-2 mb-2">
        <div class="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-brand-text-primary">
          <Sliders class="w-3.5 h-3.5 text-brand-text-primary" />
          <span>{i18n.t('audioPipeline.stageProcessing', {}, 'Processing')}</span>
        </div>
        {#if isBitPerfect}
          <span class="px-2 py-0.5 text-[10px] font-semibold tracking-wider bg-brand-accent/15 text-brand-text-primary border border-brand-accent/30 rounded-full flex items-center gap-1">
            <Sparkles class="w-2.5 h-2.5 text-brand-text-primary" />
            {i18n.t('audioPipeline.bitPerfect', {}, 'Bit-perfect')}
          </span>
        {:else}
          <span class="px-2 py-0.5 text-[10px] font-semibold tracking-wider bg-brand-accent/15 text-brand-text-primary border border-brand-accent/30 rounded-full">
            {i18n.t('audioPipeline.processed', {}, 'Processed')}
          </span>
        {/if}
      </div>

      <div class="grid grid-cols-1 gap-2 text-xs bg-brand-bg/40 p-2.5 rounded-lg border border-brand-border/30">
        <!-- Equalizer -->
        <div class="flex items-center justify-between gap-2">
          <span class="text-brand-text-secondary font-medium">{i18n.t('audioPipeline.equalizer', {}, 'Equalizer & DSP')}</span>
          <span class="font-semibold text-right text-brand-text-primary">
            {#if pipeline.eq_enabled}
              {i18n.t('audioPipeline.equalizerActive', { mode: pipeline.eq_mode || 'Active', bands: pipeline.eq_active_bands_count }, `${pipeline.eq_mode || 'Active'} (${pipeline.eq_active_bands_count} active)`)}
            {:else}
              {i18n.t('audioPipeline.equalizerDisabled', {}, 'Bypass / Disabled')}
            {/if}
          </span>
        </div>

        <!-- Volume Normalization -->
        <div class="flex items-center justify-between gap-2">
          <span class="text-brand-text-secondary font-medium">{i18n.t('audioPipeline.volumeNormalization', {}, 'Volume Normalization')}</span>
          <span class="font-semibold text-right text-brand-text-primary">
            {#if pipeline.loudness_source !== 'disabled'}
              {i18n.t('audioPipeline.normalizationGain', { gain: ((pipeline.loudness_gain_db ?? 0) >= 0 ? `+${(pipeline.loudness_gain_db ?? 0).toFixed(1)}` : (pipeline.loudness_gain_db ?? 0).toFixed(1)), source: pipeline.loudness_source }, `${(pipeline.loudness_gain_db ?? 0) >= 0 ? '+' : ''}${(pipeline.loudness_gain_db ?? 0).toFixed(1)} dB (${pipeline.loudness_source})`)}
            {:else}
              {i18n.t('audioPipeline.normalizationDisabled', {}, 'Disabled')}
            {/if}
          </span>
        </div>

        <!-- Headroom / Bit-perfect note -->
        <div class="flex items-center justify-between gap-2 pt-1 border-t border-brand-border/20 text-[11px]">
          <span class="text-brand-text-secondary font-medium">{i18n.t('audioPipeline.headroom', {}, 'Processing Mode')}</span>
          <span class="text-brand-text-primary font-mono font-semibold">
            {pipeline.headroom}
          </span>
        </div>
      </div>
    </div>

    <!-- Stage 3: Output -->
    <div>
      <div class="flex items-center justify-between gap-2 mb-2">
        <div class="flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-brand-text-primary">
          <Speaker class="w-3.5 h-3.5 text-brand-text-primary" />
          <span>{i18n.t('audioPipeline.stageOutput', {}, 'Output')}</span>
        </div>
        {#if !pipeline.resample_rate || pipeline.resample_rate === pipeline.output_sample_rate}
          <span class="px-2 py-0.5 text-[10px] font-semibold tracking-wider bg-brand-accent/15 text-brand-text-primary border border-brand-accent/30 rounded-full">
            {i18n.t('audioPipeline.directRate', {}, 'Direct rate')}
          </span>
        {:else}
          <span class="px-2 py-0.5 text-[10px] font-semibold tracking-wider bg-brand-accent/15 text-brand-text-primary border border-brand-accent/30 rounded-full">
            {i18n.t('audioPipeline.resampled', {}, 'Resampled')}
          </span>
        {/if}
      </div>

      <div class="grid grid-cols-2 gap-x-3 gap-y-1.5 text-xs bg-brand-bg/40 p-2.5 rounded-lg border border-brand-border/30">
        <div class="col-span-2 flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.outputDevice', {}, 'Device')}</span>
          <span class="text-brand-text-primary font-semibold truncate" title={pipeline.output_device_name}>
            {pipeline.output_device_name || 'Default Output Device'}
          </span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.outputFormat', {}, 'Format')}</span>
          <span class="text-brand-text-primary font-semibold">
            {formatSampleRate(pipeline.output_sample_rate)} / {pipeline.output_format}
          </span>
        </div>

        <div class="flex flex-col">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.channels', {}, 'Channels')}</span>
          <span class="text-brand-text-primary font-semibold">
            {formatChannels(pipeline.output_channels)}
          </span>
        </div>

        <div class="col-span-2 flex flex-col pt-1 border-t border-brand-border/20">
          <span class="text-[10px] uppercase font-semibold text-brand-text-secondary">{i18n.t('audioPipeline.resampling', {}, 'Resampling')}</span>
          <span class="text-brand-text-primary text-[11px] font-medium">
            {#if !pipeline.resample_rate || pipeline.resample_rate === pipeline.output_sample_rate}
              {i18n.t('audioPipeline.resamplingDirect', {}, 'Direct (no resampling)')}
            {:else}
              {i18n.t('audioPipeline.resamplingTo', { rate: formatSampleRate(pipeline.output_sample_rate) }, `Resampled to ${formatSampleRate(pipeline.output_sample_rate)}`)}
            {/if}
          </span>
        </div>

        <div class="col-span-2 flex items-center justify-between text-[11px] pt-1 text-brand-text-primary">
          <span class="text-brand-text-secondary font-medium">{i18n.t('audioPipeline.outputBackend', {}, 'Backend')}</span>
          <span class="font-mono font-semibold">{pipeline.output_backend}</span>
        </div>
      </div>
    </div>
  {/if}
</div>
