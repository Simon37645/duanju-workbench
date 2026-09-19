import { createRouter, createWebHashHistory } from "vue-router";

const router = createRouter({
  // 桌面端用 hash 路由，避免 tauri 自定义协议下的路径问题
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: "home",
      component: () => import("@/views/HomeView.vue"),
    },
    {
      path: "/w/:panel",
      name: "workspace",
      component: () => import("@/views/WorkspaceView.vue"),
      props: true,
    },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});

export default router;
