using System;
using System.CodeDom.Compiler;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.ApplicationServices;

namespace WebCheck.My;

[GeneratedCode("MyTemplate", "11.0.0.0")]
[EditorBrowsable(EditorBrowsableState.Never)]
internal class MyApplication : WindowsFormsApplicationBase
{
	[MethodImpl(MethodImplOptions.NoInlining | MethodImplOptions.NoOptimization)]
	[STAThread]
	[DebuggerHidden]
	[EditorBrowsable(EditorBrowsableState.Advanced)]
	internal static void Main(string[] Args)
	{
		Application.SetCompatibleTextRenderingDefault(WindowsFormsApplicationBase.UseCompatibleTextRendering);
		((WindowsFormsApplicationBase)WebCheck.My.MyProject.Application).Run(Args);
	}

	[DebuggerStepThrough]
	public MyApplication()
		: base((AuthenticationMode)0)
	{
		((WindowsFormsApplicationBase)this).IsSingleInstance = false;
		((WindowsFormsApplicationBase)this).EnableVisualStyles = true;
		((WindowsFormsApplicationBase)this).SaveMySettingsOnExit = true;
		((WindowsFormsApplicationBase)this).ShutdownStyle = (ShutdownMode)0;
	}

	[DebuggerStepThrough]
	protected override void OnCreateMainForm()
	{
		((WindowsFormsApplicationBase)this).MainForm = (Form)(object)WebCheck.My.MyProject.Forms.Form1;
	}
}
