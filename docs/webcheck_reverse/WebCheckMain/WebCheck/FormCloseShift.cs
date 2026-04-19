using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormCloseShift : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Bok")]
	private Button _Bok;

	[CompilerGenerated]
	[AccessedThroughProperty("ACS")]
	private CheckBox _ACS;

	internal virtual Button Bok
	{
		[CompilerGenerated]
		get
		{
			return _Bok;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Bok_Click;
			Button bok = _Bok;
			if (bok != null)
			{
				((Control)bok).Click -= eventHandler;
			}
			_Bok = value;
			bok = _Bok;
			if (bok != null)
			{
				((Control)bok).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ComboBox2")]
	internal virtual ComboBox ComboBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ComboBox1")]
	internal virtual ComboBox ComboBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ComboBox3")]
	internal virtual ComboBox ComboBox3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox ACS
	{
		[CompilerGenerated]
		get
		{
			return _ACS;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ACS_CheckedChanged;
			CheckBox aCS = _ACS;
			if (aCS != null)
			{
				aCS.CheckedChanged -= eventHandler;
			}
			_ACS = value;
			aCS = _ACS;
			if (aCS != null)
			{
				aCS.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormCloseShift()
	{
		((Form)this).Load += FormCloseShift_Load;
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
		//IL_0001: Unknown result type (might be due to invalid IL or missing references)
		//IL_000b: Expected O, but got Unknown
		//IL_000c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0016: Expected O, but got Unknown
		//IL_0017: Unknown result type (might be due to invalid IL or missing references)
		//IL_0021: Expected O, but got Unknown
		//IL_0022: Unknown result type (might be due to invalid IL or missing references)
		//IL_002c: Expected O, but got Unknown
		//IL_002d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0037: Expected O, but got Unknown
		//IL_0038: Unknown result type (might be due to invalid IL or missing references)
		//IL_0042: Expected O, but got Unknown
		//IL_0043: Unknown result type (might be due to invalid IL or missing references)
		//IL_004d: Expected O, but got Unknown
		//IL_004e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0058: Expected O, but got Unknown
		//IL_0059: Unknown result type (might be due to invalid IL or missing references)
		//IL_0063: Expected O, but got Unknown
		//IL_0064: Unknown result type (might be due to invalid IL or missing references)
		//IL_006e: Expected O, but got Unknown
		//IL_0096: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a0: Expected O, but got Unknown
		//IL_012a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0134: Expected O, but got Unknown
		//IL_01ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_01b5: Expected O, but got Unknown
		//IL_0229: Unknown result type (might be due to invalid IL or missing references)
		//IL_0233: Expected O, but got Unknown
		//IL_0322: Unknown result type (might be due to invalid IL or missing references)
		//IL_032c: Expected O, but got Unknown
		//IL_03b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_03c0: Expected O, but got Unknown
		//IL_043a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0444: Expected O, but got Unknown
		//IL_04bf: Unknown result type (might be due to invalid IL or missing references)
		//IL_04c9: Expected O, but got Unknown
		//IL_0540: Unknown result type (might be due to invalid IL or missing references)
		//IL_054a: Expected O, but got Unknown
		//IL_05c5: Unknown result type (might be due to invalid IL or missing references)
		//IL_05cf: Expected O, but got Unknown
		Bok = new Button();
		ComboBox2 = new ComboBox();
		ComboBox1 = new ComboBox();
		ComboBox3 = new ComboBox();
		GroupBox1 = new GroupBox();
		Label3 = new Label();
		Label2 = new Label();
		Label1 = new Label();
		ACS = new CheckBox();
		Label4 = new Label();
		((Control)GroupBox1).SuspendLayout();
		((Control)this).SuspendLayout();
		((Control)Bok).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Bok).Location = new Point(12, 292);
		((Control)Bok).Name = "Bok";
		((Control)Bok).Size = new Size(461, 35);
		((Control)Bok).TabIndex = 30;
		((ButtonBase)Bok).Text = "Ок";
		((ButtonBase)Bok).UseVisualStyleBackColor = true;
		ComboBox2.DropDownStyle = (ComboBoxStyle)2;
		((Control)ComboBox2).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)ComboBox2).FormattingEnabled = true;
		((Control)ComboBox2).Location = new Point(320, 35);
		((Control)ComboBox2).Name = "ComboBox2";
		((Control)ComboBox2).Size = new Size(121, 37);
		((Control)ComboBox2).TabIndex = 33;
		ComboBox1.DropDownStyle = (ComboBoxStyle)2;
		((Control)ComboBox1).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)ComboBox1).FormattingEnabled = true;
		((Control)ComboBox1).Location = new Point(89, 35);
		((Control)ComboBox1).Name = "ComboBox1";
		((Control)ComboBox1).Size = new Size(121, 37);
		((Control)ComboBox1).TabIndex = 32;
		ComboBox3.DropDownStyle = (ComboBoxStyle)2;
		((Control)ComboBox3).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)ComboBox3).FormattingEnabled = true;
		((Control)ComboBox3).Location = new Point(13, 118);
		((Control)ComboBox3).Name = "ComboBox3";
		((Control)ComboBox3).Size = new Size(428, 37);
		((Control)ComboBox3).TabIndex = 34;
		((Control)GroupBox1).Controls.Add((Control)(object)Label3);
		((Control)GroupBox1).Controls.Add((Control)(object)Label2);
		((Control)GroupBox1).Controls.Add((Control)(object)Label1);
		((Control)GroupBox1).Controls.Add((Control)(object)ComboBox1);
		((Control)GroupBox1).Controls.Add((Control)(object)ComboBox3);
		((Control)GroupBox1).Controls.Add((Control)(object)ComboBox2);
		((Control)GroupBox1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox1).Location = new Point(12, 103);
		((Control)GroupBox1).Name = "GroupBox1";
		((Control)GroupBox1).Size = new Size(461, 174);
		((Control)GroupBox1).TabIndex = 35;
		GroupBox1.TabStop = false;
		GroupBox1.Text = "Налаштування";
		Label3.AutoSize = true;
		((Control)Label3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label3).Location = new Point(8, 90);
		((Control)Label3).Name = "Label3";
		((Control)Label3).Size = new Size(335, 25);
		((Control)Label3).TabIndex = 38;
		Label3.Text = "Провести службову видачу готівки";
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(227, 41);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(87, 25);
		((Control)Label2).TabIndex = 37;
		Label2.Text = "Хвилин:";
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(8, 41);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(75, 25);
		((Control)Label1).TabIndex = 36;
		Label1.Text = "Годин:";
		((ButtonBase)ACS).AutoSize = true;
		((Control)ACS).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ACS).Location = new Point(25, 26);
		((Control)ACS).Name = "ACS";
		((Control)ACS).Size = new Size(130, 29);
		((Control)ACS).TabIndex = 36;
		((ButtonBase)ACS).Text = "Увімкнути";
		((ButtonBase)ACS).UseVisualStyleBackColor = true;
		((Control)Label4).Font = new Font("Microsoft Sans Serif", 9f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label4).Location = new Point(173, 9);
		((Control)Label4).Name = "Label4";
		((Control)Label4).Size = new Size(300, 91);
		((Control)Label4).TabIndex = 39;
		Label4.Text = "Увага! Закриття зміни можливе лише на ПК з установленим ПРРО. Якщо ПК не працює — зміну закрити неможливо.";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(488, 339);
		((Control)this).Controls.Add((Control)(object)Label4);
		((Control)this).Controls.Add((Control)(object)ACS);
		((Control)this).Controls.Add((Control)(object)GroupBox1);
		((Control)this).Controls.Add((Control)(object)Bok);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormCloseShift";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Налаштування автоматичного закриття зміни";
		((Control)GroupBox1).ResumeLayout(false);
		((Control)GroupBox1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormCloseShift_Load(object sender, EventArgs e)
	{
		((Form)this).AcceptButton = (IButtonControl)(object)Bok;
		int num = 0;
		checked
		{
			do
			{
				if (num > 9)
				{
					ComboBox1.Items.Add((object)num.ToString());
				}
				else
				{
					ComboBox1.Items.Add((object)("0" + num));
				}
				num++;
			}
			while (num <= 23);
			num = 0;
			do
			{
				if (num > 9)
				{
					ComboBox2.Items.Add((object)num.ToString());
				}
				else
				{
					ComboBox2.Items.Add((object)("0" + num));
				}
				num += 5;
			}
			while (num <= 55);
			ComboBox3.Items.Add((object)"вимкнено");
			ComboBox3.Items.Add((object)"увімкнено");
			TimeIni();
			((Control)GroupBox1).Enabled = ACS.Checked;
		}
	}

	private void TimeIni()
	{
		string text = All.f.StringGetFn(All.A.FN, "shiftclosetime");
		if (Versioned.IsNumeric((object)text))
		{
			TimeSpan timeSpan = TimeSpan.FromMinutes(Conversions.ToDouble(text));
			if (timeSpan.Days > 0)
			{
				ACS.Checked = false;
			}
			else
			{
				ACS.Checked = true;
			}
			ComboBox1.Text = timeSpan.Hours.ToString();
			ComboBox2.Text = timeSpan.Minutes.ToString();
			if (Operators.CompareString(ComboBox1.Text.Trim(), "", false) == 0)
			{
				ComboBox1.Text = "00";
			}
			if (Operators.CompareString(ComboBox2.Text.Trim(), "", false) == 0)
			{
				ComboBox2.Text = "00";
			}
		}
		else
		{
			ACS.Checked = false;
			ComboBox1.Text = "00";
			ComboBox2.Text = "00";
		}
		if (All.f.IntegerGetFn(All.A.FN, "shiftCashInOut") == 1)
		{
			ComboBox3.Text = "увімкнено";
		}
		else
		{
			ComboBox3.Text = "вимкнено";
		}
	}

	private void Bok_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(ComboBox3.Text, "увімкнено", false) == 0)
		{
			All.f.IntigerWriteFN(All.A.FN, "shiftCashInOut", 1);
		}
		else
		{
			All.f.IntigerWriteFN(All.A.FN, "shiftCashInOut", 0);
		}
		if (Operators.CompareString(ComboBox1.Text.Trim(), "", false) == 0)
		{
			ComboBox1.Text = "00";
		}
		if (Operators.CompareString(ComboBox2.Text.Trim(), "", false) == 0)
		{
			ComboBox2.Text = "00";
		}
		if (ACS.Checked)
		{
			int num = checked(Conversions.ToInteger(ComboBox1.Text.Trim()) * 60 + Conversions.ToInteger(ComboBox2.Text.Trim()));
			All.f.StringWriteFN(All.A.FN, "shiftclosetime", num.ToString());
		}
		else
		{
			All.f.StringWriteFN(All.A.FN, "shiftclosetime", "");
		}
		((Form)this).Close();
	}

	private void ACS_CheckedChanged(object sender, EventArgs e)
	{
		((Control)GroupBox1).Enabled = ACS.Checked;
	}
}
