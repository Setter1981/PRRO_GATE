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
			EventHandler eventHandler = LCB_SelectedIndexChanged;
			EventHandler eventHandler2 = LCB_DoubleClick;
			CheckedListBox lCB = _LCB;
			if (lCB != null)
			{
				((ListBox)lCB).SelectedIndexChanged -= eventHandler;
				((Control)lCB).DoubleClick -= eventHandler2;
			}
			_LCB = value;
			lCB = _LCB;
			if (lCB != null)
			{
				((ListBox)lCB).SelectedIndexChanged += eventHandler;
				((Control)lCB).DoubleClick += eventHandler2;
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

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormWebchekFooterView()
	{
		((Form)this).Load += FormWebchekFooterView_Load;
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
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_0112: Unknown result type (might be due to invalid IL or missing references)
		//IL_011c: Expected O, but got Unknown
		//IL_01a1: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ab: Expected O, but got Unknown
		//IL_0229: Unknown result type (might be due to invalid IL or missing references)
		//IL_0233: Expected O, but got Unknown
		//IL_02c0: Unknown result type (might be due to invalid IL or missing references)
		//IL_02ca: Expected O, but got Unknown
		//IL_03ba: Unknown result type (might be due to invalid IL or missing references)
		//IL_03c4: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormWebchekFooterView));
		TB = new TextBox();
		LCB = new CheckedListBox();
		NoB = new Button();
		OkB = new Button();
		Label1 = new Label();
		((Control)this).SuspendLayout();
		((TextBoxBase)TB).BackColor = SystemColors.Window;
		((Control)TB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TB).Location = new Point(516, 12);
		TB.Multiline = true;
		((Control)TB).Name = "TB";
		((TextBoxBase)TB).ReadOnly = true;
		TB.ScrollBars = (ScrollBars)2;
		((Control)TB).Size = new Size(633, 531);
		((Control)TB).TabIndex = 3;
		((Control)TB).TabStop = false;
		((ListBox)LCB).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)LCB).FormattingEnabled = true;
		((Control)LCB).ImeMode = (ImeMode)0;
		((Control)LCB).Location = new Point(12, 81);
		((Control)LCB).Name = "LCB";
		((Control)LCB).Size = new Size(474, 464);
		((Control)LCB).TabIndex = 2;
		((Control)LCB).TabStop = false;
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(12, 560);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(224, 40);
		((Control)NoB).TabIndex = 10;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(925, 560);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(224, 40);
		((Control)OkB).TabIndex = 9;
		((ButtonBase)OkB).Text = "Вибрати";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(8, 17);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(401, 48);
		((Control)Label1).TabIndex = 11;
		Label1.Text = "Вибір розділу додаткової інформації на чек,\r\nяка виводиться наприкінці чека";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1169, 612);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)TB);
		((Control)this).Controls.Add((Control)(object)LCB);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Control)this).Name = "FormWebchekFooterView";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "ВебЧек Додатковa Інформація";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
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
			LCB.Items.Add((object)"Без додаткової інформації", true);
			((ListBox)LCB).SelectedIndex = 0;
		}
		else
		{
			LCB.Items.Add((object)"Без додаткової інформації", false);
		}
		int num = CFS.IndexMaxFn();
		for (int i = 1; i <= num; i = checked(i + 1))
		{
			if (IP == i)
			{
				LCB.Items.Add((object)CFS.NameFn(i), true);
				((ListBox)LCB).SelectedIndex = i;
			}
			else
			{
				LCB.Items.Add((object)CFS.NameFn(i), false);
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
			if (((ListBox)lCB).SelectedIndex >= 0)
			{
				int num = ((ObjectCollection)lCB.Items).Count - 1;
				for (int i = 0; i <= num; i++)
				{
					lCB.SetItemChecked(i, false);
				}
				lCB.SetItemChecked(((ListBox)lCB).SelectedIndex, true);
				IP = ((ListBox)lCB).SelectedIndex;
				LoadText(((ListBox)lCB).SelectedItem.ToString());
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
				text = Strings.Replace(text, "<", "", 1, -1, (CompareMethod)0);
				text = Strings.Replace(text, ">", "", 1, -1, (CompareMethod)0);
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
		((Form)this).Close();
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		All.f.WriteInteger("Global", "ChekFooterSection", IP);
		IPm = IP;
		((Form)this).Close();
	}
}
