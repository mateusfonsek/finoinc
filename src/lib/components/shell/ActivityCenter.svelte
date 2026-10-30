<script lang="ts">
  import DiscoveryItem from "./DiscoveryItem.svelte";
  import EnrichmentItem from "./EnrichmentItem.svelte";
  import UpdateItem from "./UpdateItem.svelte";
  import { activity } from "$lib/stores/activity.svelte";
  import { watch } from "$lib/stores/watch.svelte";
  import { update } from "$lib/stores/update.svelte";

  // Background work stacks here. Progress sits on top because it is ephemeral
  // and leaves on its own; a discovered statement sits below, closer to hand,
  // because it demands a decision.
  let hasEnrichment = $derived(activity.visible);
  let hasDiscovery = $derived(watch.discoveries.length > 0);
  // Lowest priority: it waits for no decision and is gone once dismissed.
  let hasUpdate = $derived(update.found !== null);
</script>

<!-- A parallel notification, not modal: no scrim, because it interrupts
     nothing — the user carries on with what they were doing. Same decision the
     WatchToast already made; the container merely started accepting more than
     one item. -->
{#if hasEnrichment || hasDiscovery || hasUpdate}
  <div class="fixed bottom-5 right-5 z-40 flex flex-col items-end gap-2">
    {#if hasUpdate}
      <UpdateItem />
    {/if}
    {#if hasEnrichment}
      <EnrichmentItem />
    {/if}
    {#if hasDiscovery}
      <DiscoveryItem />
    {/if}
  </div>
{/if}
