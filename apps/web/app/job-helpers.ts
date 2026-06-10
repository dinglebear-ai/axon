import type { CheckSummary, DoctorService, PanelDoctorResponse, PanelStatusResponse, ServiceJob } from './panel-types';

export function savedMessage(message: string): boolean {
  return message.toLowerCase().includes('saved');
}

export function collectDoctorServices(doctor: PanelDoctorResponse | null): Array<DoctorService & { name: string }> {
  const services = doctor?.payload.services ?? {};
  return Object.entries(services).map(([name, service]) => ({
    name: name.replaceAll('_', ' '),
    ...service
  }));
}

export function doctorCheckSummary(services: Array<DoctorService & { name: string }>): CheckSummary {
  return services.reduce(
    (summary, service) => {
      if (service.ok === false) summary.error += 1;
      else summary.ok += 1;
      summary.total += 1;
      return summary;
    },
    { ok: 0, warn: 0, error: 0, skipped: 0, total: 0 }
  );
}

export function collectJobs(status: PanelStatusResponse | null): ServiceJob[] {
  if (!status) return [];
  return [
    ...withJobKind('crawl', status.payload.local_crawl_jobs),
    ...withJobKind('extract', status.payload.local_extract_jobs),
    ...withJobKind('embed', status.payload.local_embed_jobs),
    ...withJobKind('ingest', status.payload.local_ingest_jobs)
  ].sort((left, right) => Date.parse(right.updated_at) - Date.parse(left.updated_at));
}

export function withJobKind(kind: ServiceJob['kind'], jobs: ServiceJob[] | undefined): ServiceJob[] {
  return (jobs ?? []).map((job) => ({ ...job, kind }));
}

export function jobSummary(jobs: ServiceJob[]): CheckSummary {
  return jobs.reduce(
    (summary, job) => {
      if (job.status === 'failed' || job.status === 'canceled') summary.error += 1;
      else if (job.status === 'running') summary.ok += 1;
      else if (job.status === 'pending') summary.warn += 1;
      else summary.skipped += 1;
      summary.total += 1;
      return summary;
    },
    { ok: 0, warn: 0, error: 0, skipped: 0, total: 0 }
  );
}

export function normalizeJobStatus(status: string): string {
  if (status === 'completed') return 'ok';
  if (status === 'running') return 'ok';
  if (status === 'pending') return 'warn';
  if (status === 'failed' || status === 'canceled') return 'error';
  return 'skipped';
}

export function jobTargetFromUrls(value: unknown): string | null {
  if (Array.isArray(value) && value.length > 0 && typeof value[0] === 'string') return value[0];
  return null;
}

export function jobKindLabel(kind: ServiceJob['kind']): string {
  if (!kind) return 'Job';
  return kind.charAt(0).toUpperCase() + kind.slice(1);
}
