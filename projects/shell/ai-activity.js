function actions(state) {
  if (state === "queued") return [{op:"cancel",label:"Cancel"},{op:"next",label:"Do next"}]
  if (state === "running" || state === "retrying") return [{op:"cancel",label:"Cancel"}]
  if (state === "failed") return [{op:"retry",label:"Retry"},{op:"release",label:"Dismiss"}]
  return []
}

function rows(values, now) {
  return (Array.isArray(values) ? values : []).filter(function(job) {
    return ["queued","running","retrying","failed"].indexOf(job.state) >= 0
      || (["succeeded","cancelled","superseded"].indexOf(job.state) >= 0 && now - Number(job.updated) < 5)
  }).map(function(job) {
    // Explicit metadata projection: never retain a request or result field.
    return {id:String(job.id), consumer:String(job.consumer), label:String(job.label),
      state:String(job.state), created:Number(job.created), updated:Number(job.updated),
      model:String(job.model), attempts:Number(job.attempts), queueDuration:Number(job.queueDuration),
      tokens:{input:Number((job.tokens || {}).input || 0),output:Number((job.tokens || {}).output || 0)},
      error:String(job.error || "")}
  })
}

function indicator(jobs) {
  if (jobs.some(function(j) { return j.state === "failed" })) return "failed"
  if (jobs.some(function(j) { return ["queued","running","retrying"].indexOf(j.state) >= 0 })) return "active"
  return ""
}
