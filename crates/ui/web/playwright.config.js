import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir:'./tests',timeout:30000,workers:1,
  use:{channel:'msedge',headless:true,baseURL:'http://127.0.0.1:4173',viewport:{width:1100,height:760}},
  webServer:{command:'node test-server.mjs',url:'http://127.0.0.1:4173',reuseExistingServer:false},
});
