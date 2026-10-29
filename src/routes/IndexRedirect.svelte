<script lang="ts">
  import { onMount } from "svelte";
  import { push, router } from "svelte-spa-router";
  import Loading from "$lib/components/ui/Loading.svelte";
  import { listTransactions } from "$lib/api/transactions";

  let checking = $state(true);

  /** The check is async: a file opened from Finder may already have taken the
   *  user to Import, and redirecting then would drop the statement. */
  function leave(to: string) {
    if (router.location === "/") push(to);
  }

  onMount(async () => {
    try {
      const some = await listTransactions({
        account_id: null,
        month: null,
        category_id: null,
        q: null,
        limit: 1,
      });
      leave(some.length === 0 ? "/onboarding" : "/dashboard");
    } catch {
      leave("/onboarding");
    } finally {
      checking = false;
    }
  });
</script>

{#if checking}
  <!-- A decision route: gone in milliseconds. A centred indicator avoids the
       flash of stray corner text this used to show. -->
  <div class="h-full grid place-items-center">
    <Loading />
  </div>
{/if}
