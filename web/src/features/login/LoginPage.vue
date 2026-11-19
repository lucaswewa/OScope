<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import { useSession } from "@/api/connection/session";
import { ApiError } from "@/api/errors";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The login (W1.3), when the server has accounts. Then back to where the
 * user was, or to the start. A session the server ended says so.
 */
const { t } = useI18n();
const session = useSession();
const route = useRoute();
const router = useRouter();
const name = ref("");
const password = ref("");
const wrong = ref<string>();
const busy = ref(false);

async function login(): Promise<void> {
  busy.value = true;
  wrong.value = undefined;
  try {
    await session.login(name.value, password.value);
    password.value = "";
    const then = typeof route.query.then === "string" ? route.query.then : "/";
    await router.replace(then);
  } catch (thrown) {
    wrong.value =
      thrown instanceof ApiError && thrown.status === 401
        ? t("login.wrong")
        : t("login.failed", { why: thrown instanceof Error ? thrown.message : String(thrown) });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <main class="login">
    <form class="form" @submit.prevent="login">
      <h1>{{ t("login.title") }}</h1>
      <Banner v-if="session.ended" level="info">{{ t("login.ended") }}</Banner>
      <Field :label="t('login.name')">
        <TextField v-model="name" autocomplete="username" />
      </Field>
      <Field :label="t('login.password')" :error="wrong">
        <TextField v-model="password" secret autocomplete="current-password" />
      </Field>
      <Button variant="primary" type="submit" :busy="busy">{{ t("login.go") }}</Button>
    </form>
  </main>
</template>

<style scoped>
.login {
  display: grid;
  place-items: center;
  min-block-size: 100vh;
  padding: var(--space-6);
}

.form {
  display: grid;
  gap: var(--space-4);
  inline-size: min(100%, 24rem);
}

h1 {
  margin: 0;
}
</style>
