export interface SamplePickerEvents<T> {
  loading: (loading: boolean) => void;
  loaded: (sample: T) => void;
  failed: (message: string) => void;
}

/** Fetching a sample is asynchronous, and applying one resets the job: a compile started in that
 *  window would submit the old source and then lose its job. So the picker says when it is loading
 *  (the shell holds Compile until it is not), and only the newest pick may land. Free of React. */
export function createSamplePicker<T>(fetchSample: (id: string) => Promise<T>, on: SamplePickerEvents<T>) {
  let seq = 0;
  return {
    async pick(id: string) {
      const mine = ++seq;
      on.loading(true);
      try {
        const sample = await fetchSample(id);
        if (mine === seq) on.loaded(sample);
      } catch (e) {
        if (mine === seq) on.failed(e instanceof Error ? e.message : String(e));
      } finally {
        if (mine === seq) on.loading(false);
      }
    },
    dispose() { seq++; },
  };
}
