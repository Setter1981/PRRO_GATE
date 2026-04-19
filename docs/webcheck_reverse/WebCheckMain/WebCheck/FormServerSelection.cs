using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormServerSelection : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Servers")]
	private CheckedListBox _Servers;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	private bool NewBas;

	internal virtual CheckedListBox Servers
	{
		[CompilerGenerated]
		get
		{
			return _Servers;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Servers_SelectedIndexChanged;
			CheckedListBox servers = _Servers;
			if (servers != null)
			{
				((ListBox)servers).SelectedIndexChanged -= eventHandler;
			}
			_Servers = value;
			servers = _Servers;
			if (servers != null)
			{
				((ListBox)servers).SelectedIndexChanged += eventHandler;
			}
		}
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_005a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0064: Expected O, but got Unknown
		//IL_00d1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00db: Expected O, but got Unknown
		//IL_0158: Unknown result type (might be due to invalid IL or missing references)
		//IL_0162: Expected O, but got Unknown
		//IL_0242: Unknown result type (might be due to invalid IL or missing references)
		//IL_024c: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormServerSelection));
		Servers = new CheckedListBox();
		NoB = new Button();
		OkB = new Button();
		((Control)this).SuspendLayout();
		Servers.CheckOnClick = true;
		((ListBox)Servers).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)Servers).FormattingEnabled = true;
		((Control)Servers).Location = new Point(12, 12);
		((Control)Servers).Name = "Servers";
		((Control)Servers).Size = new Size(422, 379);
		((Control)Servers).TabIndex = 0;
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(12, 406);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(132, 40);
		((Control)NoB).TabIndex = 5;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(303, 406);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(132, 40);
		((Control)OkB).TabIndex = 6;
		((ButtonBase)OkB).Text = "Вибрати";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(447, 458);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)Servers);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormServerSelection";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Налаштування АЦСК";
		((Control)this).ResumeLayout(false);
	}

	public FormServerSelection(bool NewBase)
	{
		((Form)this).Load += FormServerSelection_Load;
		NewBas = NewBase;
		InitializeComponent();
	}

	private void FormServerSelection_Load(object sender, EventArgs e)
	{
		((Form)this).AcceptButton = (IButtonControl)(object)OkB;
		((Form)this).CancelButton = (IButtonControl)(object)NoB;
		LoadServers();
	}

	private void LoadServers()
	{
		int count = All.SF.Servers(0).Count;
		for (int i = 0; i <= count; i = checked(i + 1))
		{
			((ObjectCollection)Servers.Items).Add((object)All.SF.Servers(i).Name);
			_ = null;
		}
		if (NewBas)
		{
			((ListBox)Servers).SelectedIndex = All.A.AcskSettingsTemp;
		}
		else
		{
			((ListBox)Servers).SelectedIndex = All.A.AcskSettings;
		}
	}

	private void Servers_SelectedIndexChanged(object sender, EventArgs e)
	{
		checked
		{
			if (((ListBox)Servers).SelectedIndex >= 0)
			{
				int num = ((ObjectCollection)Servers.Items).Count - 1;
				for (int i = 0; i <= num; i++)
				{
					Servers.SetItemChecked(i, false);
				}
				Servers.SetItemChecked(((ListBox)Servers).SelectedIndex, true);
			}
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		if (NewBas)
		{
			All.A.AcskSettingsTemp = ((ListBox)Servers).SelectedIndex;
		}
		else if (All.A.Status && ((ListBox)Servers).SelectedIndex != All.A.AcskSettings)
		{
			All.A.AcskSettings = ((ListBox)Servers).SelectedIndex;
			All.A.AcskSettingsTemp = All.A.AcskSettings;
			All.f.StringWriteFN(All.A.FN, "Acsksettings", Conversions.ToString(All.A.AcskSettings));
		}
		((Form)this).Close();
	}
}
