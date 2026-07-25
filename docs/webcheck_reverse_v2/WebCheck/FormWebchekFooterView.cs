using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormWebchekFooterView : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("LCB")]
	private CheckedListBox _LCB;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	private int IP;

	private int IPm;

	private WordWord WW;

	private IniHGB CFS;

	[field: AccessedThroughProperty("TB")]
	internal virtual TextBox TB
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckedListBox LCB
	{
		[CompilerGenerated]
		get
		{
			return _LCB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = LCB_SelectedIndexChanged;
			EventHandler value3 = LCB_DoubleClick;
			CheckedListBox lCB = _LCB;
			if (lCB != null)
			{
				lCB.SelectedIndexChanged -= value2;
				lCB.DoubleClick -= value3;
			}
			_LCB = value;
			lCB = _LCB;
			if (lCB != null)
			{
				lCB.SelectedIndexChanged += value2;
				lCB.DoubleClick += value3;
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
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
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
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormWebchekFooterView()
	{
		base.Load += FormWebchekFooterView_Load;
		IP = 0;
		IPm = 0;
		WW = new WordWord();
		CFS = new IniHGB(All.MyDoc() + "\\WebCheck\\Logo\\ChekFooterSection.ini");
		InitializeComponent();
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormWebchekFooterView));
		this.TB = new System.Windows.Forms.TextBox();
		this.LCB = new System.Windows.Forms.CheckedListBox();
		this.NoB = new System.Windows.Forms.Button();
		this.OkB = new System.Windows.Forms.Button();
		this.Label1 = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.TB.BackColor = System.Drawing.SystemColors.Window;
		this.TB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TB.Location = new System.Drawing.Point(516, 12);
		this.TB.Multiline = true;
		this.TB.Name = "TB";
		this.TB.ReadOnly = true;
		this.TB.ScrollBars = System.Windows.Forms.ScrollBars.Vertical;
		this.TB.Size = new System.Drawing.Size(633, 531);
		this.TB.TabIndex = 3;
		this.TB.TabStop = false;
		this.LCB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LCB.FormattingEnabled = true;
		this.LCB.ImeMode = System.Windows.Forms.ImeMode.NoControl;
		this.LCB.Location = new System.Drawing.Point(12, 81);
		this.LCB.Name = "LCB";
		this.LCB.Size = new System.Drawing.Size(474, 464);
		this.LCB.TabIndex = 2;
		this.LCB.TabStop = false;
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(12, 560);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(224, 40);
		this.NoB.TabIndex = 10;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(925, 560);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(224, 40);
		this.OkB.TabIndex = 9;
		this.OkB.Text = "Вибрати";
		this.OkB.UseVisualStyleBackColor = true;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(8, 17);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(401, 48);
		this.Label1.TabIndex = 11;
		this.Label1.Text = "Вибір розділу додаткової інформації на чек,\r\nяка виводиться наприкінці чека";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1169, 612);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.TB);
		base.Controls.Add(this.LCB);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.Name = "FormWebchekFooterView";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "ВебЧек Додатковa Інформація";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormWebchekFooterView_Load(object sender, EventArgs e)
	{
		IP = All.f.GetInteger("Global", "ChekFooterSection", 0);
		if (IP == 0)
		{
			All.f.WriteInteger("Global", "ChekFooterSection", -1);
			IP = -1;
		}
		IPm = IP;
		if (IP < 1)
		{
			LCB.Items.Add("Без додаткової інформації", isChecked: true);
			LCB.SelectedIndex = 0;
		}
		else
		{
			LCB.Items.Add("Без додаткової інформації", isChecked: false);
		}
		int num = CFS.IndexMaxFn();
		for (int i = 1; i <= num; i = checked(i + 1))
		{
			if (IP == i)
			{
				LCB.Items.Add(CFS.NameFn(i), isChecked: true);
				LCB.SelectedIndex = i;
			}
			else
			{
				LCB.Items.Add(CFS.NameFn(i), isChecked: false);
			}
		}
	}

	private void LCB_SelectedIndexChanged(object sender, EventArgs e)
	{
		SelectLCBone();
	}

	private void LCB_DoubleClick(object sender, EventArgs e)
	{
		SelectLCBone();
	}

	private void SelectLCBone()
	{
		CheckedListBox lCB = LCB;
		checked
		{
			if (lCB.SelectedIndex >= 0)
			{
				int num = lCB.Items.Count - 1;
				for (int i = 0; i <= num; i++)
				{
					lCB.SetItemChecked(i, value: false);
				}
				lCB.SetItemChecked(lCB.SelectedIndex, value: true);
				IP = lCB.SelectedIndex;
				LoadText(lCB.SelectedItem.ToString());
				lCB = null;
			}
		}
	}

	private void LoadText(string e)
	{
		TB.Text = "";
		int num = 1;
		do
		{
			string text = CFS.StringGetFn(e.ToString(), num.ToString()).Trim();
			if (text.Length >= 4)
			{
				text = Strings.Replace(text, "<", "");
				text = Strings.Replace(text, ">", "");
				text = num + ".   " + text;
				TextBox tB;
				(tB = TB).Text = tB.Text + text + Environment.NewLine + Environment.NewLine;
				num = checked(num + 1);
				continue;
			}
			break;
		}
		while (num <= 999);
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		All.f.WriteInteger("Global", "ChekFooterSection", IP);
		IPm = IP;
		Close();
	}
}
