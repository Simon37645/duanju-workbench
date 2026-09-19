import { defineStore } from "pinia";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "@/api/ipc";
import { EVT_JOB } from "@/api/events";
import type { Job } from "@/types/models";

interface JobEvent {
  type: "reset" | "update";
  jobs?: Job[];
  job?: Job;
}

interface State {
  jobs: Job[];
  _unlisten: UnlistenFn | null;
  barOpen: boolean;
}

export const useJobsStore = defineStore("jobs", {
  state: (): State => ({
    jobs: [],
    _unlisten: null,
    barOpen: false,
  }),

  getters: {
    running: (s) =>
      s.jobs.filter((j) => j.status === "running" || j.status === "queued"),
    recent: (s) => s.jobs.slice(0, 30),
    byId: (s) => (id: string | null | undefined) =>
      id ? s.jobs.find((j) => j.id === id) ?? null : null,
  },

  actions: {
    bind() {
      if (this._unlisten) return;
      listen<JobEvent>(EVT_JOB, (ev) => {
        const p = ev.payload;
        if (p.type === "reset" && p.jobs) {
          this.jobs = p.jobs;
        } else if (p.type === "update" && p.job) {
          const i = this.jobs.findIndex((j) => j.id === p.job!.id);
          const isNew = i < 0;
          if (i >= 0) this.jobs[i] = p.job;
          else this.jobs.unshift(p.job);
          // 有新任务开始跑就自动展开，跑完不自动收起（用户可能想看结果）
          if (isNew && (p.job.status === "running" || p.job.status === "queued")) {
            this.barOpen = true;
          }
        }
      }).then((fn) => {
        this._unlisten = fn;
      });
      this.refresh();
    },

    async refresh() {
      this.jobs = await api.jobsList();
    },

    async cancel(id: string) {
      await api.jobCancel(id);
    },

    async clearFinished() {
      await api.jobsClearFinished();
      await this.refresh();
    },
  },
});
